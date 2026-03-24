use crate::ast::*;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};
use std::collections::HashMap;
use std::process::Command;

mod init;
mod structs;
mod functions;
mod stmts;
mod exprs;
mod calls;
mod http;
mod json;

pub(super) struct Codegen<'ctx> {
    pub(super) context: &'ctx Context,
    pub(super) module: Module<'ctx>,
    pub(super) builder: Builder<'ctx>,
    pub(super) variables: HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    pub(super) global_variables: HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    pub(super) current_function: Option<FunctionValue<'ctx>>,
    pub(super) printf: Option<FunctionValue<'ctx>>,
    pub(super) scanf: Option<FunctionValue<'ctx>>,
    pub(super) loop_stack: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
    pub(super) map_variables: HashMap<String, (Type, Type)>,
    // Global map variables: need runtime calloc init + tracking across functions
    pub(super) global_map_variables: HashMap<String, (Type, Type)>,
    // Element type for array parameters (passed as pointer to first element)
    pub(super) array_params: HashMap<String, BasicTypeEnum<'ctx>>,
    // LLVM struct types keyed by struct name
    pub(super) struct_defs: HashMap<String, (StructType<'ctx>, Vec<String>)>, // name → (llvm_type, field_names)
    // Original AST field types keyed by struct name
    pub(super) struct_field_types: HashMap<String, Vec<Type>>,
    // Tracks which local variables are structs (maps var_name → struct_name)
    pub(super) var_struct_names: HashMap<String, String>,
}

impl<'ctx> Codegen<'ctx> {
    fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        Codegen {
            context,
            module,
            builder,
            variables: HashMap::new(),
            global_variables: HashMap::new(),
            current_function: None,
            printf: None,
            scanf: None,
            loop_stack: Vec::new(),
            map_variables: HashMap::new(),
            global_map_variables: HashMap::new(),
            array_params: HashMap::new(),
            struct_defs: HashMap::new(),
            struct_field_types: HashMap::new(),
            var_struct_names: HashMap::new(),
        }
    }

    pub(super) fn convert_type(&self, typ: &Type) -> BasicTypeEnum<'ctx> {
        match typ {
            Type::I32  => self.context.i32_type().into(),
            Type::I64  => self.context.i64_type().into(),
            Type::F32  => self.context.f32_type().into(),
            Type::F64  => self.context.f64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            Type::Str  => self.context.i8_type().ptr_type(AddressSpace::default()).into(),
            Type::Array { element, size } => match self.convert_type(element) {
                BasicTypeEnum::IntType(t)     => t.array_type(*size as u32).into(),
                BasicTypeEnum::FloatType(t)   => t.array_type(*size as u32).into(),
                BasicTypeEnum::PointerType(t) => t.array_type(*size as u32).into(), // [str; N]
                _ => panic!("Arrays of this element type not supported"),
            },
            Type::Map { .. } => self.context.i8_type().ptr_type(AddressSpace::default()).into(),
            Type::Void => panic!("void cannot be used as a variable type"),
            Type::Struct(name) => {
                let (st, _) = self.struct_defs.get(name)
                    .unwrap_or_else(|| panic!("Unknown struct type '{}'", name));
                (*st).into()
            }
        }
    }

    pub(super) fn build_fn_type(&self, return_type: BasicTypeEnum<'ctx>, params: &[BasicMetadataTypeEnum<'ctx>]) -> FunctionType<'ctx> {
        match return_type {
            BasicTypeEnum::IntType(t)    => t.fn_type(params, false),
            BasicTypeEnum::FloatType(t)  => t.fn_type(params, false),
            BasicTypeEnum::PointerType(t) => t.fn_type(params, false),
            BasicTypeEnum::StructType(t) => t.fn_type(params, false),
            _ => panic!("Functions cannot return this type"),
        }
    }

    // Auto-widen i32 → i64 when storing into wider type (keeps backward compat)
    pub(super) fn coerce_int(&self, val: BasicValueEnum<'ctx>, target: BasicTypeEnum<'ctx>) -> BasicValueEnum<'ctx> {
        if let (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(tt)) = (val, target) {
            if iv.get_type().get_bit_width() < tt.get_bit_width() {
                return self.builder.build_int_s_extend(iv, tt, "widen").unwrap().into();
            }
        }
        val
    }

    pub(super) fn block_terminated(&self) -> bool {
        self.builder.get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
    }

    fn verify(&self) -> bool {
        match self.module.verify() {
            Ok(_) => true,
            Err(e) => {
                eprintln!("=== LLVM verification error ===");
                eprintln!("{}", e.to_string());
                false
            }
        }
    }

    fn write_to_file(&self, path: &str) -> Result<(), String> {
        self.module.print_to_file(path)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn generate(
    program: &Program,
    module_name: &str,
    tmp_prefix: &str,
    exe_path: &str,
    link_extras: &[String],
    verbose: bool,
) -> Result<(), String> {
    let context = Context::create();
    let mut codegen = Codegen::new(&context, module_name);

    codegen.generate(program)?;

    if !codegen.verify() {
        return Err("Module verification failed".to_string());
    }

    if verbose {
        eprintln!("=== LLVM IR ===");
        eprintln!("{}", codegen.module.print_to_string().to_string());
    }

    // Write .ll to /tmp
    let ll_path  = format!("{}.ll", tmp_prefix);
    let obj_path = format!("{}.o",  tmp_prefix);
    codegen.write_to_file(&ll_path)?;

    // Compile .ll → .o
    let llc_status = Command::new("llc")
        .args(&["-filetype=obj", "-relocation-model=pic", &ll_path, "-o", &obj_path])
        .status()
        .map_err(|e| format!("Failed to run llc: {}", e))?;

    if !llc_status.success() {
        return Err(format!("llc failed with exit code {:?}", llc_status.code()));
    }

    // Link .o → binary
    let mut clang_args: Vec<&str> = vec![&obj_path, "-o", exe_path, "-no-pie"];
    for extra in link_extras {
        clang_args.push(extra.as_str());
    }
    let clang_status = Command::new("clang")
        .args(&clang_args)
        .status()
        .map_err(|e| format!("Failed to run clang: {}", e))?;

    if !clang_status.success() {
        return Err(format!("clang failed with exit code {:?}", clang_status.code()));
    }

    eprintln!("Compiled → {}", exe_path);
    Ok(())
}
