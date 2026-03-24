use crate::ast::*;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use super::Codegen;

impl<'ctx> Codegen<'ctx> {
    fn generate_globals(&mut self, globals: &[crate::ast::GlobalVar]) -> Result<(), String> {
        for g in globals {
            let llvm_type = self.convert_type(&g.typ);
            let global = self.module.add_global(llvm_type, None, &g.name);

            // Set initializer (must be a constant)
            match &g.initializer {
                None => {
                    global.set_initializer(&llvm_type.const_zero());
                }
                Some(Expression::IntLiteral(n)) => {
                    if let BasicTypeEnum::IntType(t) = llvm_type {
                        global.set_initializer(&t.const_int(*n as u64, true));
                    } else {
                        return Err(format!("Global '{}': integer literal for non-integer type", g.name));
                    }
                }
                Some(Expression::FloatLiteral(v)) => {
                    if let BasicTypeEnum::FloatType(t) = llvm_type {
                        global.set_initializer(&t.const_float(*v));
                    } else {
                        return Err(format!("Global '{}': float literal for non-float type", g.name));
                    }
                }
                Some(Expression::ArrayLiteral(elems)) => {
                    if let BasicTypeEnum::ArrayType(at) = llvm_type {
                        let elem_type = at.get_element_type();
                        let const_elems: Result<Vec<_>, _> = elems.iter().map(|e| {
                            match e {
                                Expression::IntLiteral(n) => {
                                    if let BasicTypeEnum::IntType(t) = elem_type {
                                        Ok(t.const_int(*n as u64, true).into())
                                    } else { Err("Type mismatch in global array literal".to_string()) }
                                }
                                Expression::FloatLiteral(v) => {
                                    if let BasicTypeEnum::FloatType(t) = elem_type {
                                        Ok(t.const_float(*v).into())
                                    } else { Err("Type mismatch in global array literal".to_string()) }
                                }
                                _ => Err("Global array initializers must be literals".to_string()),
                            }
                        }).collect();
                        match (elem_type, const_elems?) {
                            (BasicTypeEnum::IntType(t), vals) => {
                                let iv: Vec<_> = vals.iter().map(|v: &BasicValueEnum| v.into_int_value()).collect();
                                global.set_initializer(&t.const_array(&iv));
                            }
                            (BasicTypeEnum::FloatType(t), vals) => {
                                let fv: Vec<_> = vals.iter().map(|v: &BasicValueEnum| v.into_float_value()).collect();
                                global.set_initializer(&t.const_array(&fv));
                            }
                            _ => return Err("Unsupported global array element type".to_string()),
                        }
                    } else {
                        return Err(format!("Global '{}': array literal for non-array type", g.name));
                    }
                }
                Some(_) => {
                    return Err(format!("Global '{}': only literal initializers supported for globals", g.name));
                }
            }

            let ptr = global.as_pointer_value();
            self.global_variables.insert(g.name.clone(), (ptr, llvm_type));

            // Track global maps so functions can use map_set/map_get/map_has on them
            if let Type::Map { key, value } = &g.typ {
                self.global_map_variables.insert(g.name.clone(), (*key.clone(), *value.clone()));
            }
        }
        Ok(())
    }

    /// Generates __vit_global_init() — callocs every global map variable.
    /// Called automatically at the start of main().
    fn build_global_map_init(&mut self) {
        let void_type = self.context.void_type();
        let i64_type  = self.context.i64_type();
        let i8_ptr    = self.context.i8_type().ptr_type(AddressSpace::default());
        let calloc_fn = self.module.get_function("calloc").unwrap();

        let fn_val = self.module.add_function(
            "__vit_global_init",
            void_type.fn_type(&[], false),
            None,
        );
        let entry = self.context.append_basic_block(fn_val, "entry");
        self.builder.position_at_end(entry);

        // Allocate each global map
        let globals: Vec<(String, Type, Type)> = self.global_map_variables
            .iter()
            .map(|(name, (k, v))| (name.clone(), k.clone(), v.clone()))
            .collect();

        for (name, key, val) in globals {
            let alloc_size: u64 = match (&key, &val) {
                (Type::I32, Type::I32) => 49152,
                (Type::Str, Type::I32) => 65536,
                (Type::I32, Type::I64) => 65536,
                (Type::I64, Type::I32) => 65536,
                (Type::I64, Type::I64) => 81920,
                (Type::Str, Type::Str) => 81920,
                _ => 65536,
            };
            let mem = self.builder
                .build_call(
                    calloc_fn,
                    &[i64_type.const_int(1, false).into(), i64_type.const_int(alloc_size, false).into()],
                    "map_mem",
                )
                .unwrap()
                .try_as_basic_value()
                .left()
                .unwrap();

            // Store calloc result into the global variable
            let (global_ptr, _) = self.global_variables[&name];
            self.builder.build_store(global_ptr, mem).unwrap();
        }

        self.builder.build_return(None).unwrap();
    }

    fn generate(&mut self, program: &Program) -> Result<(), String> {
        self.register_strbuf_type();         // must come before generate_struct_defs
        self.generate_struct_defs(&program.structs);
        self.declare_printf();
        self.declare_scanf();
        self.declare_string_builtins();
        self.declare_math_builtins();
        self.build_request_alloc_helpers();
        self.build_sort_comparators();
        self.build_strbuf_helpers();         // needs malloc / strlen / memcpy / realloc / free
        self.build_vit_add();
        self.build_vit_remove();
        self.build_vit_replace();
        self.build_vit_split();
        self.build_map_helpers();
        self.declare_net_builtins();
        self.build_tcp_helpers();
        self.declare_http_route_table();
        self.declare_extern_functions(&program.externs);
        self.generate_globals(&program.globals)?;
        self.build_global_map_init();  // generates __vit_global_init() for global maps

        for function in &program.functions {
            self.generate_function(function)?;
        }

        Ok(())
    }

    fn generate_function(&mut self, function: &Function) -> Result<(), String> {
        let i8_ptr = self.context.i8_type().ptr_type(AddressSpace::default());

        // Build parameter types — arrays and structs are passed as pointer
        let i8_ptr = self.context.i8_type().ptr_type(AddressSpace::default());
        let param_types: Vec<BasicMetadataTypeEnum> = function
            .parameters
            .iter()
            .map(|p| {
                match &p.typ {
                    Type::Array { element, .. } => {
                        let elem = self.convert_type(element);
                        match elem {
                            BasicTypeEnum::IntType(t)     => t.ptr_type(AddressSpace::default()).into(),
                            BasicTypeEnum::FloatType(t)   => t.ptr_type(AddressSpace::default()).into(),
                            BasicTypeEnum::PointerType(t) => t.ptr_type(AddressSpace::default()).into(),
                            _ => panic!("Unsupported array element type in parameter"),
                        }
                    }
                    Type::Struct(sname) => {
                        let (st, _) = self.struct_defs.get(sname)
                            .unwrap_or_else(|| panic!("Unknown struct '{}'", sname));
                        st.ptr_type(AddressSpace::default()).into()
                    }
                    Type::Map { .. } => {
                        // Maps are passed as i8* (pointer to the calloc'd backing store)
                        i8_ptr.into()
                    }
                    _ => self.convert_type(&p.typ).into(),
                }
            })
            .collect();

        // Build function type
        let return_type = self.convert_type(&function.return_type);
        let fn_type = self.build_fn_type(return_type, &param_types);

        // Add function to module
        let fn_value = self.module.add_function(&function.name, fn_type, None);
        self.current_function = Some(fn_value);

        // Create entry block
        let entry = self.context.append_basic_block(fn_value, "entry");
        self.builder.position_at_end(entry);

        // Clear locals and pre-populate with globals
        self.variables.clear();
        self.map_variables.clear();
        self.array_params.clear();
        self.var_struct_names.clear();
        for (name, &val) in &self.global_variables {
            self.variables.insert(name.clone(), val);
        }
        // Make global maps visible to map_set/map_get/map_has
        for (name, types) in &self.global_map_variables.clone() {
            self.map_variables.insert(name.clone(), types.clone());
        }
        // In main(), initialize all global maps via __vit_global_init()
        if function.name == "main" {
            if let Some(init_fn) = self.module.get_function("__vit_global_init") {
                self.builder.build_call(init_fn, &[], "").unwrap();
            }
        }

        // Allocate parameters
        for (i, param) in function.parameters.iter().enumerate() {
            let param_value = fn_value.get_nth_param(i as u32).unwrap();
            param_value.set_name(&param.name);

            match &param.typ {
                Type::Array { element, .. } => {
                    // Array param: incoming value is a pointer to first element
                    let elem_type = self.convert_type(element);
                    let alloca = self.builder.build_alloca(i8_ptr, &param.name).unwrap();
                    self.builder.build_store(alloca, param_value).unwrap();
                    self.variables.insert(param.name.clone(), (alloca, i8_ptr.into()));
                    self.array_params.insert(param.name.clone(), elem_type);
                }
                Type::Struct(sname) => {
                    let (st, _) = self.struct_defs.get(sname)
                        .unwrap_or_else(|| panic!("Unknown struct '{}'", sname))
                        .clone();
                    let src_ptr = param_value.into_pointer_value();
                    if sname == "StrBuf" {
                        // StrBuf uses pointer semantics: mutations to len/data must be
                        // visible in the caller. Store the caller's pointer directly.
                        self.variables.insert(param.name.clone(), (src_ptr, st.into()));
                    } else {
                        // Other structs: copy into a local alloca (pass-by-value).
                        let alloca = self.builder.build_alloca(st, &param.name).unwrap();
                        let struct_val = self.builder.build_load(st, src_ptr, "param_struct").unwrap();
                        self.builder.build_store(alloca, struct_val).unwrap();
                        self.variables.insert(param.name.clone(), (alloca, st.into()));
                    }
                    self.var_struct_names.insert(param.name.clone(), sname.clone());
                }
                Type::Map { key, value } => {
                    // Map param: incoming value is i8* (pointer to backing store)
                    // Store it in a local alloca so map_has/map_get/map_set can load it uniformly
                    let alloca = self.builder.build_alloca(i8_ptr, &param.name).unwrap();
                    self.builder.build_store(alloca, param_value).unwrap();
                    self.variables.insert(param.name.clone(), (alloca, i8_ptr.into()));
                    self.map_variables.insert(param.name.clone(), (*key.clone(), *value.clone()));
                }
                _ => {
                    let typ = self.convert_type(&param.typ);
                    let alloca = self.builder.build_alloca(typ, &param.name).unwrap();
                    self.builder.build_store(alloca, param_value).unwrap();
                    self.variables.insert(param.name.clone(), (alloca, typ));
                }
            }
        }

        // Generate body
        for stmt in &function.body {
            self.generate_statement(stmt)?;
            if self.block_terminated() { break; }
        }

        Ok(())
    }
}
