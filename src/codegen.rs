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

pub struct Codegen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    variables: HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    global_variables: HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    current_function: Option<FunctionValue<'ctx>>,
    printf: Option<FunctionValue<'ctx>>,
    scanf: Option<FunctionValue<'ctx>>,
    loop_stack: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
    map_variables: HashMap<String, (Type, Type)>,
    // Global map variables: need runtime calloc init + tracking across functions
    global_map_variables: HashMap<String, (Type, Type)>,
    // Element type for array parameters (passed as pointer to first element)
    array_params: HashMap<String, BasicTypeEnum<'ctx>>,
    // LLVM struct types keyed by struct name
    struct_defs: HashMap<String, (StructType<'ctx>, Vec<String>)>, // name → (llvm_type, field_names)
    // Original AST field types keyed by struct name
    struct_field_types: HashMap<String, Vec<Type>>,
    // Tracks which local variables are structs (maps var_name → struct_name)
    var_struct_names: HashMap<String, String>,
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

    fn declare_printf(&mut self) {
        let i8_ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let printf_type = i32_type.fn_type(&[i8_ptr_type.into()], true);
        self.printf = Some(self.module.add_function("printf", printf_type, None));
    }

    fn declare_scanf(&mut self) {
        let i8_ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let scanf_type = i32_type.fn_type(&[i8_ptr_type.into()], true);
        self.scanf = Some(self.module.add_function("scanf", scanf_type, None));
    }

    fn declare_string_builtins(&mut self) {
        let i8_ptr = self.context.i8_type().ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();

        // strlen(i8*) -> i64
        self.module.add_function("strlen", i64_type.fn_type(&[i8_ptr.into()], false), None);
        // malloc(i64) -> i8*
        self.module.add_function("malloc", i8_ptr.fn_type(&[i64_type.into()], false), None);
        // strcpy(i8*, i8*) -> i8*
        self.module.add_function("strcpy", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
        // strcat(i8*, i8*) -> i8*
        self.module.add_function("strcat", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
        // strstr(i8*, i8*) -> i8*
        self.module.add_function("strstr", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
        // memcpy(i8*, i8*, i64) -> i8*
        self.module.add_function("memcpy", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into(), i64_type.into()], false), None);
        // strdup(i8*) -> i8*  (makes writable copy — strtok needs mutable input)
        self.module.add_function("strdup", i8_ptr.fn_type(&[i8_ptr.into()], false), None);
        // strtok(i8*, i8*) -> i8*
        self.module.add_function("strtok", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
        // strcmp(i8*, i8*) -> i32
        self.module.add_function("strcmp", i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
        // strncpy(i8*, i8*, i64) -> i8*
        self.module.add_function("strncpy", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into(), i64_type.into()], false), None);
        // strncmp(i8*, i8*, i64) -> i32
        self.module.add_function("strncmp", i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into(), i64_type.into()], false), None);
    }

    fn declare_math_builtins(&mut self) {
        let i8_ptr  = self.context.i8_type().ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let f64_type = self.context.f64_type();
        let void_type = self.context.void_type();

        // sqrt(f64) -> f64
        self.module.add_function("sqrt", f64_type.fn_type(&[f64_type.into()], false), None);
        // pow(f64, f64) -> f64
        self.module.add_function("pow", f64_type.fn_type(&[f64_type.into(), f64_type.into()], false), None);
        // atoi(i8*) -> i32
        self.module.add_function("atoi", i32_type.fn_type(&[i8_ptr.into()], false), None);
        // atol(i8*) -> i64
        self.module.add_function("atol", i64_type.fn_type(&[i8_ptr.into()], false), None);
        // atof(i8*) -> f64
        self.module.add_function("atof", f64_type.fn_type(&[i8_ptr.into()], false), None);
        // sprintf(i8*, i8*, ...) -> i32
        self.module.add_function("sprintf", i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], true), None);
        // qsort(i8*, i64, i64, i8*) -> void
        self.module.add_function("qsort", void_type.fn_type(&[i8_ptr.into(), i64_type.into(), i64_type.into(), i8_ptr.into()], false), None);
        // calloc(i64, i64) -> i8*
        self.module.add_function("calloc", i8_ptr.fn_type(&[i64_type.into(), i64_type.into()], false), None);
        // realloc(i8*, i64) -> i8*
        self.module.add_function("realloc", i8_ptr.fn_type(&[i8_ptr.into(), i64_type.into()], false), None);
        // free(i8*) -> void
        self.module.add_function("free", void_type.fn_type(&[i8_ptr.into()], false), None);
    }

    /// Pre-registers the built-in StrBuf struct type: { i8*, i64, i64 } (data, len, cap).
    /// Must be called before generate_struct_defs so user structs can have StrBuf fields.
    fn register_strbuf_type(&mut self) {
        let i8_ptr   = self.context.i8_type().ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let strbuf_type = self.context.struct_type(
            &[i8_ptr.into(), i64_type.into(), i64_type.into()],
            false,
        );
        self.struct_defs.insert(
            "StrBuf".to_string(),
            (strbuf_type, vec!["data".to_string(), "len".to_string(), "cap".to_string()]),
        );
    }

    /// Builds strbuf_new / strbuf_append / strbuf_to_str / strbuf_len / strbuf_free.
    /// Must be called after declare_string_builtins (needs malloc / strlen / memcpy / realloc / free).
    fn build_strbuf_helpers(&mut self) {
        let i8_type  = self.context.i8_type();
        let i8_ptr   = i8_type.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let void_type = self.context.void_type();
        let (strbuf_type, _) = self.struct_defs["StrBuf"].clone();
        let strbuf_ptr = strbuf_type.ptr_type(AddressSpace::default());

        let malloc_fn   = self.module.get_function("malloc").unwrap();
        let strlen_fn   = self.module.get_function("strlen").unwrap();
        let memcpy_fn   = self.module.get_function("memcpy").unwrap();
        let realloc_fn  = self.module.get_function("realloc").unwrap();
        let free_fn     = self.module.get_function("free").unwrap();

        // ------------------------------------------------------------------
        // strbuf_new() -> StrBuf   (returns by value)
        // ------------------------------------------------------------------
        {
            let fn_val = self.module.add_function(
                "strbuf_new",
                strbuf_type.fn_type(&[], false),
                None,
            );
            let entry = self.context.append_basic_block(fn_val, "entry");
            self.builder.position_at_end(entry);

            let init_cap = i64_type.const_int(64, false);
            let data = self.builder
                .build_call(malloc_fn, &[init_cap.into()], "data")
                .unwrap()
                .try_as_basic_value()
                .left()
                .unwrap()
                .into_pointer_value();

            let mut agg = strbuf_type.const_zero();
            agg = self.builder.build_insert_value(agg, data, 0, "s0").unwrap().into_struct_value();
            agg = self.builder.build_insert_value(agg, i64_type.const_int(0, false), 1, "s1").unwrap().into_struct_value();
            agg = self.builder.build_insert_value(agg, init_cap, 2, "s2").unwrap().into_struct_value();
            self.builder.build_return(Some(&agg)).unwrap();
        }

        // ------------------------------------------------------------------
        // strbuf_append(buf: StrBuf*, s: i8*) -> void
        // ------------------------------------------------------------------
        {
            let fn_val = self.module.add_function(
                "strbuf_append",
                void_type.fn_type(&[strbuf_ptr.into(), i8_ptr.into()], false),
                None,
            );
            let buf_ptr = fn_val.get_nth_param(0).unwrap().into_pointer_value();
            let s_ptr   = fn_val.get_nth_param(1).unwrap().into_pointer_value();

            let entry   = self.context.append_basic_block(fn_val, "entry");
            let grow_bb = self.context.append_basic_block(fn_val, "grow");
            let copy_bb = self.context.append_basic_block(fn_val, "copy");
            self.builder.position_at_end(entry);

            // Load fields
            let data_field = self.builder.build_struct_gep(strbuf_type, buf_ptr, 0, "dp").unwrap();
            let len_field  = self.builder.build_struct_gep(strbuf_type, buf_ptr, 1, "lp").unwrap();
            let cap_field  = self.builder.build_struct_gep(strbuf_type, buf_ptr, 2, "cp").unwrap();

            let len = self.builder.build_load(i64_type, len_field, "len").unwrap().into_int_value();
            let cap = self.builder.build_load(i64_type, cap_field, "cap").unwrap().into_int_value();

            let slen = self.builder.build_call(strlen_fn, &[s_ptr.into()], "slen")
                .unwrap().try_as_basic_value().left().unwrap().into_int_value();

            // needed = len + slen + 1
            let tmp    = self.builder.build_int_add(len, slen, "tmp").unwrap();
            let needed = self.builder.build_int_add(tmp, i64_type.const_int(1, false), "needed").unwrap();

            let cond = self.builder.build_int_compare(IntPredicate::UGT, needed, cap, "need_grow").unwrap();
            self.builder.build_conditional_branch(cond, grow_bb, copy_bb).unwrap();

            // grow block
            self.builder.position_at_end(grow_bb);
            let new_cap = self.builder.build_int_mul(needed, i64_type.const_int(2, false), "new_cap").unwrap();
            let old_data = self.builder.build_load(i8_ptr, data_field, "old_data").unwrap().into_pointer_value();
            let new_data = self.builder.build_call(realloc_fn, &[old_data.into(), new_cap.into()], "new_data")
                .unwrap().try_as_basic_value().left().unwrap().into_pointer_value();
            self.builder.build_store(data_field, new_data).unwrap();
            self.builder.build_store(cap_field, new_cap).unwrap();
            self.builder.build_unconditional_branch(copy_bb).unwrap();

            // copy block
            self.builder.position_at_end(copy_bb);
            let data = self.builder.build_load(i8_ptr, data_field, "data").unwrap().into_pointer_value();
            let dest = unsafe { self.builder.build_gep(i8_type, data, &[len], "dest") }.unwrap();
            let copy_len = self.builder.build_int_add(slen, i64_type.const_int(1, false), "cplen").unwrap();
            self.builder.build_call(memcpy_fn, &[dest.into(), s_ptr.into(), copy_len.into()], "").unwrap();
            let new_len = self.builder.build_int_add(len, slen, "new_len").unwrap();
            self.builder.build_store(len_field, new_len).unwrap();
            self.builder.build_return(None).unwrap();
        }

        // ------------------------------------------------------------------
        // strbuf_to_str(buf: StrBuf*) -> i8*
        // ------------------------------------------------------------------
        {
            let fn_val = self.module.add_function(
                "strbuf_to_str",
                i8_ptr.fn_type(&[strbuf_ptr.into()], false),
                None,
            );
            let buf_ptr = fn_val.get_nth_param(0).unwrap().into_pointer_value();
            let entry = self.context.append_basic_block(fn_val, "entry");
            self.builder.position_at_end(entry);
            let data_field = self.builder.build_struct_gep(strbuf_type, buf_ptr, 0, "dp").unwrap();
            let data = self.builder.build_load(i8_ptr, data_field, "data").unwrap();
            self.builder.build_return(Some(&data)).unwrap();
        }

        // ------------------------------------------------------------------
        // strbuf_len(buf: StrBuf*) -> i64
        // ------------------------------------------------------------------
        {
            let fn_val = self.module.add_function(
                "strbuf_len",
                i64_type.fn_type(&[strbuf_ptr.into()], false),
                None,
            );
            let buf_ptr = fn_val.get_nth_param(0).unwrap().into_pointer_value();
            let entry = self.context.append_basic_block(fn_val, "entry");
            self.builder.position_at_end(entry);
            let len_field = self.builder.build_struct_gep(strbuf_type, buf_ptr, 1, "lp").unwrap();
            let len = self.builder.build_load(i64_type, len_field, "len").unwrap();
            self.builder.build_return(Some(&len)).unwrap();
        }

        // ------------------------------------------------------------------
        // strbuf_free(buf: StrBuf*) -> void
        // ------------------------------------------------------------------
        {
            let fn_val = self.module.add_function(
                "strbuf_free",
                void_type.fn_type(&[strbuf_ptr.into()], false),
                None,
            );
            let buf_ptr = fn_val.get_nth_param(0).unwrap().into_pointer_value();
            let entry = self.context.append_basic_block(fn_val, "entry");
            self.builder.position_at_end(entry);
            let data_field = self.builder.build_struct_gep(strbuf_type, buf_ptr, 0, "dp").unwrap();
            let data = self.builder.build_load(i8_ptr, data_field, "data").unwrap().into_pointer_value();
            self.builder.build_call(free_fn, &[data.into()], "").unwrap();
            self.builder.build_return(None).unwrap();
        }
    }

    fn build_request_alloc_helpers(&mut self) {
        let i8_type = self.context.i8_type();
        let i8_ptr = i8_type.ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let void_type = self.context.void_type();
        let cap = 8192u64;
        let arr_type = i8_ptr.array_type(cap as u32);

        let malloc_fn = self.module.get_function("malloc").unwrap();
        let free_fn = self.module.get_function("free").unwrap();
        let strdup_fn = self.module.get_function("strdup").unwrap();

        let allocs_g = self.module.get_global("__vit_req_allocs").unwrap_or_else(|| {
            let g = self.module.add_global(arr_type, None, "__vit_req_allocs");
            g.set_initializer(&arr_type.const_zero());
            g
        });
        let count_g = self.module.get_global("__vit_req_alloc_count").unwrap_or_else(|| {
            let g = self.module.add_global(i32_type, None, "__vit_req_alloc_count");
            g.set_initializer(&i32_type.const_zero());
            g
        });
        let enabled_g = self.module.get_global("__vit_req_alloc_enabled").unwrap_or_else(|| {
            let g = self.module.add_global(i32_type, None, "__vit_req_alloc_enabled");
            g.set_initializer(&i32_type.const_zero());
            g
        });
        let allocs_ptr = allocs_g.as_pointer_value();
        let count_ptr = count_g.as_pointer_value();
        let enabled_ptr = enabled_g.as_pointer_value();

        {
            let f = self.module.add_function("__vit_req_begin", void_type.fn_type(&[], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(entry);
            self.builder.build_store(count_ptr, i32_type.const_zero()).unwrap();
            self.builder.build_store(enabled_ptr, i32_type.const_int(1, false)).unwrap();
            self.builder.build_return(None).unwrap();
        }

        {
            let f = self.module.add_function("__vit_req_track", i8_ptr.fn_type(&[i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let ret_bb = self.context.append_basic_block(f, "ret");
            let do_track_bb = self.context.append_basic_block(f, "track");
            self.builder.position_at_end(entry);

            let ptr = f.get_nth_param(0).unwrap().into_pointer_value();
            let ptr_is_null = self.builder.build_is_null(ptr, "ptr_is_null").unwrap();
            let enabled = self.builder.build_load(i32_type, enabled_ptr, "enabled").unwrap().into_int_value();
            let is_enabled = self.builder.build_int_compare(IntPredicate::NE, enabled, i32_type.const_zero(), "is_enabled").unwrap();
            let ptr_not_null = self.builder.build_int_compare(
                IntPredicate::EQ,
                ptr_is_null,
                self.context.bool_type().const_zero(),
                "ptr_not_null"
            ).unwrap();
            let can_try = self.builder.build_and(ptr_not_null, is_enabled, "can_try").unwrap();
            self.builder.build_conditional_branch(can_try, do_track_bb, ret_bb).unwrap();

            self.builder.position_at_end(do_track_bb);
            let count = self.builder.build_load(i32_type, count_ptr, "count").unwrap().into_int_value();
            let under_cap = self.builder.build_int_compare(
                IntPredicate::ULT,
                count,
                i32_type.const_int(cap, false),
                "under_cap"
            ).unwrap();
            let store_bb = self.context.append_basic_block(f, "store");
            self.builder.build_conditional_branch(under_cap, store_bb, ret_bb).unwrap();

            self.builder.position_at_end(store_bb);
            let zero = i32_type.const_zero();
            let slot = unsafe { self.builder.build_gep(arr_type, allocs_ptr, &[zero, count], "slot") }.unwrap();
            self.builder.build_store(slot, ptr).unwrap();
            let next = self.builder.build_int_add(count, i32_type.const_int(1, false), "next").unwrap();
            self.builder.build_store(count_ptr, next).unwrap();
            self.builder.build_unconditional_branch(ret_bb).unwrap();

            self.builder.position_at_end(ret_bb);
            self.builder.build_return(Some(&ptr)).unwrap();
        }

        {
            let f = self.module.add_function("__vit_req_free", void_type.fn_type(&[i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let scan_bb = self.context.append_basic_block(f, "scan");
            let body_bb = self.context.append_basic_block(f, "body");
            let next_bb = self.context.append_basic_block(f, "next");
            let after_bb = self.context.append_basic_block(f, "after");
            self.builder.position_at_end(entry);

            let target = f.get_nth_param(0).unwrap().into_pointer_value();
            let idx_ptr = self.builder.build_alloca(i32_type, "idx").unwrap();
            self.builder.build_store(idx_ptr, i32_type.const_zero()).unwrap();
            let target_is_null = self.builder.build_is_null(target, "target_is_null").unwrap();
            self.builder.build_conditional_branch(target_is_null, after_bb, scan_bb).unwrap();

            self.builder.position_at_end(scan_bb);
            let idx = self.builder.build_load(i32_type, idx_ptr, "idxv").unwrap().into_int_value();
            let count = self.builder.build_load(i32_type, count_ptr, "count").unwrap().into_int_value();
            let done = self.builder.build_int_compare(IntPredicate::UGE, idx, count, "done").unwrap();
            self.builder.build_conditional_branch(done, after_bb, body_bb).unwrap();

            self.builder.position_at_end(body_bb);
            let zero = i32_type.const_zero();
            let slot = unsafe { self.builder.build_gep(arr_type, allocs_ptr, &[zero, idx], "slot") }.unwrap();
            let tracked = self.builder.build_load(i8_ptr, slot, "tracked").unwrap().into_pointer_value();
            let same = self.builder.build_int_compare(
                IntPredicate::EQ,
                self.builder.build_ptr_to_int(tracked, i64_type, "tracked_i").unwrap(),
                self.builder.build_ptr_to_int(target, i64_type, "target_i").unwrap(),
                "same"
            ).unwrap();
            let clear_bb = self.context.append_basic_block(f, "clear");
            self.builder.build_conditional_branch(same, clear_bb, next_bb).unwrap();

            self.builder.position_at_end(clear_bb);
            self.builder.build_store(slot, i8_ptr.const_null()).unwrap();
            self.builder.build_unconditional_branch(next_bb).unwrap();

            self.builder.position_at_end(next_bb);
            let idx2 = self.builder.build_load(i32_type, idx_ptr, "idx2").unwrap().into_int_value();
            let next = self.builder.build_int_add(idx2, i32_type.const_int(1, false), "next").unwrap();
            self.builder.build_store(idx_ptr, next).unwrap();
            self.builder.build_unconditional_branch(scan_bb).unwrap();

            self.builder.position_at_end(after_bb);
            self.builder.build_call(free_fn, &[target.into()], "").unwrap();
            self.builder.build_return(None).unwrap();
        }

        {
            let f = self.module.add_function("__vit_req_free_all", void_type.fn_type(&[], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let scan_bb = self.context.append_basic_block(f, "scan");
            let body_bb = self.context.append_basic_block(f, "body");
            let next_bb = self.context.append_basic_block(f, "next");
            let after_bb = self.context.append_basic_block(f, "after");
            self.builder.position_at_end(entry);

            let idx_ptr = self.builder.build_alloca(i32_type, "idx").unwrap();
            self.builder.build_store(idx_ptr, i32_type.const_zero()).unwrap();
            self.builder.build_unconditional_branch(scan_bb).unwrap();

            self.builder.position_at_end(scan_bb);
            let idx = self.builder.build_load(i32_type, idx_ptr, "idxv").unwrap().into_int_value();
            let count = self.builder.build_load(i32_type, count_ptr, "count").unwrap().into_int_value();
            let done = self.builder.build_int_compare(IntPredicate::UGE, idx, count, "done").unwrap();
            self.builder.build_conditional_branch(done, after_bb, body_bb).unwrap();

            self.builder.position_at_end(body_bb);
            let zero = i32_type.const_zero();
            let slot = unsafe { self.builder.build_gep(arr_type, allocs_ptr, &[zero, idx], "slot") }.unwrap();
            let tracked = self.builder.build_load(i8_ptr, slot, "tracked").unwrap().into_pointer_value();
            let is_null = self.builder.build_is_null(tracked, "is_null").unwrap();
            let free_one_bb = self.context.append_basic_block(f, "free_one");
            self.builder.build_conditional_branch(is_null, next_bb, free_one_bb).unwrap();

            self.builder.position_at_end(free_one_bb);
            self.builder.build_call(free_fn, &[tracked.into()], "").unwrap();
            self.builder.build_store(slot, i8_ptr.const_null()).unwrap();
            self.builder.build_unconditional_branch(next_bb).unwrap();

            self.builder.position_at_end(next_bb);
            let idx2 = self.builder.build_load(i32_type, idx_ptr, "idx2").unwrap().into_int_value();
            let next = self.builder.build_int_add(idx2, i32_type.const_int(1, false), "next").unwrap();
            self.builder.build_store(idx_ptr, next).unwrap();
            self.builder.build_unconditional_branch(scan_bb).unwrap();

            self.builder.position_at_end(after_bb);
            self.builder.build_store(count_ptr, i32_type.const_zero()).unwrap();
            self.builder.build_return(None).unwrap();
        }

        {
            let free_all_fn = self.module.get_function("__vit_req_free_all").unwrap();
            let f = self.module.add_function("__vit_req_end", void_type.fn_type(&[], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(entry);
            self.builder.build_call(free_all_fn, &[], "").unwrap();
            self.builder.build_store(enabled_ptr, i32_type.const_zero()).unwrap();
            self.builder.build_return(None).unwrap();
        }

        {
            let track_fn = self.module.get_function("__vit_req_track").unwrap();
            let f = self.module.add_function("__vit_req_alloc", i8_ptr.fn_type(&[i64_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(entry);
            let size = f.get_nth_param(0).unwrap().into_int_value();
            let ptr = self.builder.build_call(malloc_fn, &[size.into()], "ptr").unwrap()
                .try_as_basic_value().left().unwrap().into_pointer_value();
            let tracked = self.builder.build_call(track_fn, &[ptr.into()], "tracked").unwrap()
                .try_as_basic_value().left().unwrap().into_pointer_value();
            self.builder.build_return(Some(&tracked)).unwrap();
        }

        {
            let track_fn = self.module.get_function("__vit_req_track").unwrap();
            let f = self.module.add_function("__vit_req_strdup", i8_ptr.fn_type(&[i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(entry);
            let src = f.get_nth_param(0).unwrap().into_pointer_value();
            let ptr = self.builder.build_call(strdup_fn, &[src.into()], "ptr").unwrap()
                .try_as_basic_value().left().unwrap().into_pointer_value();
            let tracked = self.builder.build_call(track_fn, &[ptr.into()], "tracked").unwrap()
                .try_as_basic_value().left().unwrap().into_pointer_value();
            self.builder.build_return(Some(&tracked)).unwrap();
        }
    }

    // Emits __vit_cmp_i32, __vit_cmp_i64, __vit_cmp_f64 for qsort
    fn build_sort_comparators(&mut self) {
        let i8_ptr   = self.context.i8_type().ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let f64_type = self.context.f64_type();

        // i32 comparator
        {
            let ft = i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
            let f  = self.module.add_function("__vit_cmp_i32", ft, None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);
            let a = self.builder.build_load(i32_type, f.get_nth_param(0).unwrap().into_pointer_value(), "a").unwrap().into_int_value();
            let b = self.builder.build_load(i32_type, f.get_nth_param(1).unwrap().into_pointer_value(), "b").unwrap().into_int_value();
            let gt = self.builder.build_int_compare(IntPredicate::SGT, a, b, "gt").unwrap();
            let lt = self.builder.build_int_compare(IntPredicate::SLT, a, b, "lt").unwrap();
            let gt_i = self.builder.build_int_z_extend(gt, i32_type, "gti").unwrap();
            let lt_i = self.builder.build_int_z_extend(lt, i32_type, "lti").unwrap();
            let res  = self.builder.build_int_sub(gt_i, lt_i, "res").unwrap();
            self.builder.build_return(Some(&res)).unwrap();
        }
        // i64 comparator
        {
            let ft = i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
            let f  = self.module.add_function("__vit_cmp_i64", ft, None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);
            let a = self.builder.build_load(i64_type, f.get_nth_param(0).unwrap().into_pointer_value(), "a").unwrap().into_int_value();
            let b = self.builder.build_load(i64_type, f.get_nth_param(1).unwrap().into_pointer_value(), "b").unwrap().into_int_value();
            let gt = self.builder.build_int_compare(IntPredicate::SGT, a, b, "gt").unwrap();
            let lt = self.builder.build_int_compare(IntPredicate::SLT, a, b, "lt").unwrap();
            let gt_i = self.builder.build_int_z_extend(gt, i32_type, "gti").unwrap();
            let lt_i = self.builder.build_int_z_extend(lt, i32_type, "lti").unwrap();
            let res  = self.builder.build_int_sub(gt_i, lt_i, "res").unwrap();
            self.builder.build_return(Some(&res)).unwrap();
        }
        // f64 comparator
        {
            let ft = i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
            let f  = self.module.add_function("__vit_cmp_f64", ft, None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);
            let a = self.builder.build_load(f64_type, f.get_nth_param(0).unwrap().into_pointer_value(), "a").unwrap().into_float_value();
            let b = self.builder.build_load(f64_type, f.get_nth_param(1).unwrap().into_pointer_value(), "b").unwrap().into_float_value();
            let gt = self.builder.build_float_compare(FloatPredicate::OGT, a, b, "gt").unwrap();
            let lt = self.builder.build_float_compare(FloatPredicate::OLT, a, b, "lt").unwrap();
            let gt_i = self.builder.build_int_z_extend(gt, i32_type, "gti").unwrap();
            let lt_i = self.builder.build_int_z_extend(lt, i32_type, "lti").unwrap();
            let res  = self.builder.build_int_sub(gt_i, lt_i, "res").unwrap();
            self.builder.build_return(Some(&res)).unwrap();
        }
    }

    // add(s1, s2) -> malloc + strcpy + strcat
    fn build_vit_add(&mut self) {
        let i8_ptr = self.context.i8_type().ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let function = self.module.add_function("__vit_add", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let s1 = function.get_nth_param(0).unwrap().into_pointer_value();
        let s2 = function.get_nth_param(1).unwrap().into_pointer_value();

        let strlen = self.module.get_function("strlen").unwrap();
        let malloc = self.module.get_function("__vit_req_alloc").unwrap();
        let strcpy = self.module.get_function("strcpy").unwrap();
        let strcat = self.module.get_function("strcat").unwrap();

        let n1 = self.builder.build_call(strlen, &[s1.into()], "n1").unwrap().try_as_basic_value().left().unwrap().into_int_value();
        let n2 = self.builder.build_call(strlen, &[s2.into()], "n2").unwrap().try_as_basic_value().left().unwrap().into_int_value();
        let total = self.builder.build_int_add(n1, n2, "t").unwrap();
        let total = self.builder.build_int_add(total, i64_type.const_int(1, false), "t1").unwrap();

        let result = self.builder.build_call(malloc, &[total.into()], "res").unwrap().try_as_basic_value().left().unwrap().into_pointer_value();
        self.builder.build_call(strcpy, &[result.into(), s1.into()], "").unwrap();
        self.builder.build_call(strcat, &[result.into(), s2.into()], "").unwrap();
        self.builder.build_return(Some(&result)).unwrap();
    }

    // remove(s, sub) -> strstr + malloc + memcpy + strcpy
    fn build_vit_remove(&mut self) {
        let i8_type = self.context.i8_type();
        let i8_ptr = i8_type.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let function = self.module.add_function("__vit_remove", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);

        let entry_block  = self.context.append_basic_block(function, "entry");
        let not_found    = self.context.append_basic_block(function, "not_found");
        let found        = self.context.append_basic_block(function, "found");

        self.builder.position_at_end(entry_block);
        let s   = function.get_nth_param(0).unwrap().into_pointer_value();
        let sub = function.get_nth_param(1).unwrap().into_pointer_value();

        let strstr = self.module.get_function("strstr").unwrap();
        let strlen = self.module.get_function("strlen").unwrap();
        let malloc = self.module.get_function("__vit_req_alloc").unwrap();
        let memcpy = self.module.get_function("memcpy").unwrap();
        let strcpy = self.module.get_function("strcpy").unwrap();

        let pos = self.builder.build_call(strstr, &[s.into(), sub.into()], "pos").unwrap().try_as_basic_value().left().unwrap().into_pointer_value();
        let pos_int = self.builder.build_ptr_to_int(pos, i64_type, "pi").unwrap();
        let is_null = self.builder.build_int_compare(IntPredicate::EQ, pos_int, i64_type.const_int(0, false), "is_null").unwrap();
        self.builder.build_conditional_branch(is_null, not_found, found).unwrap();

        self.builder.position_at_end(not_found);
        self.builder.build_return(Some(&s)).unwrap();

        self.builder.position_at_end(found);
        let s_len   = self.builder.build_call(strlen, &[s.into()], "sl").unwrap().try_as_basic_value().left().unwrap().into_int_value();
        let sub_len = self.builder.build_call(strlen, &[sub.into()], "subl").unwrap().try_as_basic_value().left().unwrap().into_int_value();
        let diff  = self.builder.build_int_sub(s_len, sub_len, "diff").unwrap();
        let total = self.builder.build_int_add(diff, i64_type.const_int(1, false), "tot").unwrap();

        let result = self.builder.build_call(malloc, &[total.into()], "res").unwrap().try_as_basic_value().left().unwrap().into_pointer_value();

        // prefix_len = pos - s
        let s_int      = self.builder.build_ptr_to_int(s, i64_type, "si").unwrap();
        let prefix_len = self.builder.build_int_sub(pos_int, s_int, "plen").unwrap();
        self.builder.build_call(memcpy, &[result.into(), s.into(), prefix_len.into()], "").unwrap();

        // dest = result + prefix_len
        let dest = unsafe { self.builder.build_gep(i8_type, result, &[prefix_len], "dest") }.unwrap();
        // after_sub = pos + sub_len
        let after_sub = unsafe { self.builder.build_gep(i8_type, pos, &[sub_len], "asub") }.unwrap();
        self.builder.build_call(strcpy, &[dest.into(), after_sub.into()], "").unwrap();
        self.builder.build_return(Some(&result)).unwrap();
    }

    // replace(s, old, new) -> strstr + malloc + memcpy + strcpy x2
    fn build_vit_replace(&mut self) {
        let i8_type = self.context.i8_type();
        let i8_ptr = i8_type.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let function = self.module.add_function("__vit_replace", i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into(), i8_ptr.into()], false), None);

        let entry_block = self.context.append_basic_block(function, "entry");
        let not_found   = self.context.append_basic_block(function, "not_found");
        let found       = self.context.append_basic_block(function, "found");

        self.builder.position_at_end(entry_block);
        let s   = function.get_nth_param(0).unwrap().into_pointer_value();
        let old = function.get_nth_param(1).unwrap().into_pointer_value();
        let new = function.get_nth_param(2).unwrap().into_pointer_value();

        let strstr = self.module.get_function("strstr").unwrap();
        let strlen = self.module.get_function("strlen").unwrap();
        let malloc = self.module.get_function("__vit_req_alloc").unwrap();
        let memcpy = self.module.get_function("memcpy").unwrap();
        let strcpy = self.module.get_function("strcpy").unwrap();

        let pos = self.builder.build_call(strstr, &[s.into(), old.into()], "pos").unwrap().try_as_basic_value().left().unwrap().into_pointer_value();
        let pos_int = self.builder.build_ptr_to_int(pos, i64_type, "pi").unwrap();
        let is_null = self.builder.build_int_compare(IntPredicate::EQ, pos_int, i64_type.const_int(0, false), "is_null").unwrap();
        self.builder.build_conditional_branch(is_null, not_found, found).unwrap();

        self.builder.position_at_end(not_found);
        self.builder.build_return(Some(&s)).unwrap();

        self.builder.position_at_end(found);
        let s_len   = self.builder.build_call(strlen, &[s.into()], "sl").unwrap().try_as_basic_value().left().unwrap().into_int_value();
        let old_len = self.builder.build_call(strlen, &[old.into()], "ol").unwrap().try_as_basic_value().left().unwrap().into_int_value();
        let new_len = self.builder.build_call(strlen, &[new.into()], "nl").unwrap().try_as_basic_value().left().unwrap().into_int_value();
        let diff  = self.builder.build_int_sub(s_len, old_len, "diff").unwrap();
        let base  = self.builder.build_int_add(diff, new_len, "base").unwrap();
        let total = self.builder.build_int_add(base, i64_type.const_int(1, false), "tot").unwrap();

        let result = self.builder.build_call(malloc, &[total.into()], "res").unwrap().try_as_basic_value().left().unwrap().into_pointer_value();

        // prefix
        let s_int      = self.builder.build_ptr_to_int(s, i64_type, "si").unwrap();
        let prefix_len = self.builder.build_int_sub(pos_int, s_int, "plen").unwrap();
        self.builder.build_call(memcpy, &[result.into(), s.into(), prefix_len.into()], "").unwrap();

        // copy new string after prefix
        let dest_new = unsafe { self.builder.build_gep(i8_type, result, &[prefix_len], "dn") }.unwrap();
        self.builder.build_call(strcpy, &[dest_new.into(), new.into()], "").unwrap();

        // copy suffix (after old)
        let after_old   = unsafe { self.builder.build_gep(i8_type, pos, &[old_len], "ao") }.unwrap();
        let suffix_dest = unsafe { self.builder.build_gep(i8_type, dest_new, &[new_len], "sd") }.unwrap();
        self.builder.build_call(strcpy, &[suffix_dest.into(), after_old.into()], "").unwrap();
        self.builder.build_return(Some(&result)).unwrap();
    }

    // split(s, sep, arr_ptr, max) -> i32  — fills arr with strtok tokens, returns count
    fn build_vit_split(&mut self) {
        let i8_type = self.context.i8_type();
        let i8_ptr  = i8_type.ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();

        // __vit_split(i8* s, i8* sep, i8** arr, i32 max) -> i32
        let fn_type = i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into(), i8_ptr.into(), i32_type.into()], false);
        let function = self.module.add_function("__vit_split", fn_type, None);

        let entry = self.context.append_basic_block(function, "entry");
        let cond  = self.context.append_basic_block(function, "cond");
        let body  = self.context.append_basic_block(function, "body");
        let after = self.context.append_basic_block(function, "after");

        self.builder.position_at_end(entry);
        let s   = function.get_nth_param(0).unwrap().into_pointer_value();
        let sep = function.get_nth_param(1).unwrap().into_pointer_value();
        let arr = function.get_nth_param(2).unwrap().into_pointer_value();
        let max = function.get_nth_param(3).unwrap().into_int_value();

        let strdup = self.module.get_function("__vit_req_strdup").unwrap();
        let strtok = self.module.get_function("strtok").unwrap();

        // copy = strdup(s)  — strtok needs a mutable buffer
        let copy = self.builder.build_call(strdup, &[s.into()], "copy").unwrap()
            .try_as_basic_value().left().unwrap().into_pointer_value();

        let count_ptr = self.builder.build_alloca(i32_type, "cnt").unwrap();
        self.builder.build_store(count_ptr, i32_type.const_int(0, false)).unwrap();

        let tok_ptr = self.builder.build_alloca(i8_ptr, "tok_slot").unwrap();
        let first_tok = self.builder.build_call(strtok, &[copy.into(), sep.into()], "tok0").unwrap()
            .try_as_basic_value().left().unwrap().into_pointer_value();
        self.builder.build_store(tok_ptr, first_tok).unwrap();

        self.builder.build_unconditional_branch(cond).unwrap();

        // cond: tok != NULL && count < max
        self.builder.position_at_end(cond);
        let tok   = self.builder.build_load(i8_ptr, tok_ptr, "tok").unwrap().into_pointer_value();
        let count = self.builder.build_load(i32_type, count_ptr, "cnt").unwrap().into_int_value();
        let tok_int   = self.builder.build_ptr_to_int(tok, i64_type, "ti").unwrap();
        let not_null  = self.builder.build_int_compare(IntPredicate::NE, tok_int, i64_type.const_int(0, false), "nn").unwrap();
        let under_max = self.builder.build_int_compare(IntPredicate::SLT, count, max, "um").unwrap();
        let go        = self.builder.build_and(not_null, under_max, "go").unwrap();
        self.builder.build_conditional_branch(go, body, after).unwrap();

        // body: arr[count] = tok; count++; tok = strtok(NULL, sep)
        self.builder.position_at_end(body);
        let tok   = self.builder.build_load(i8_ptr, tok_ptr, "tok").unwrap().into_pointer_value();
        let count = self.builder.build_load(i32_type, count_ptr, "cnt").unwrap().into_int_value();
        let idx   = self.builder.build_int_z_extend(count, i64_type, "idx").unwrap();
        // arr is already ptr to first element (i8**); GEP by idx to reach arr[idx]
        let slot  = unsafe { self.builder.build_gep(i8_ptr, arr, &[idx], "slot") }.unwrap();
        self.builder.build_store(slot, tok).unwrap();

        let next_count = self.builder.build_int_add(count, i32_type.const_int(1, false), "nc").unwrap();
        self.builder.build_store(count_ptr, next_count).unwrap();

        let null_ptr = i8_ptr.const_null();
        let next_tok = self.builder.build_call(strtok, &[null_ptr.into(), sep.into()], "ntok").unwrap()
            .try_as_basic_value().left().unwrap().into_pointer_value();
        self.builder.build_store(tok_ptr, next_tok).unwrap();
        self.builder.build_unconditional_branch(cond).unwrap();

        // after: return count
        self.builder.position_at_end(after);
        let final_count = self.builder.build_load(i32_type, count_ptr, "fc").unwrap();
        self.builder.build_return(Some(&final_count)).unwrap();
    }

    fn build_map_helpers(&mut self) {
        let i8_type  = self.context.i8_type();
        let i8_ptr   = i8_type.ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let void_type = self.context.void_type();
        let cap       = 4096u64;
        let cap_i32   = i32_type.const_int(cap, false);

        // ====== __vit_hash_str(i8* s) -> i32  (djb2, result & 4095) ======
        {
            let f = self.module.add_function("__vit_hash_str",
                i32_type.fn_type(&[i8_ptr.into()], false), None);
            let entry   = self.context.append_basic_block(f, "entry");
            let cond_bb = self.context.append_basic_block(f, "cond");
            let body_bb = self.context.append_basic_block(f, "body");
            let exit_bb = self.context.append_basic_block(f, "exit");

            self.builder.position_at_end(entry);
            let s        = f.get_nth_param(0).unwrap().into_pointer_value();
            let hash_ptr = self.builder.build_alloca(i64_type, "hp").unwrap();
            let pos_ptr  = self.builder.build_alloca(i64_type, "pp").unwrap();
            self.builder.build_store(hash_ptr, i64_type.const_int(5381, false)).unwrap();
            self.builder.build_store(pos_ptr,  i64_type.const_int(0, false)).unwrap();
            self.builder.build_unconditional_branch(cond_bb).unwrap();

            self.builder.position_at_end(cond_bb);
            let pos  = self.builder.build_load(i64_type, pos_ptr, "pos").unwrap().into_int_value();
            let cptr = unsafe { self.builder.build_gep(i8_type, s, &[pos], "cp") }.unwrap();
            let c    = self.builder.build_load(i8_type, cptr, "c").unwrap().into_int_value();
            let iz   = self.builder.build_int_compare(IntPredicate::EQ, c, i8_type.const_int(0, false), "iz").unwrap();
            self.builder.build_conditional_branch(iz, exit_bb, body_bb).unwrap();

            self.builder.position_at_end(body_bb);
            let hash  = self.builder.build_load(i64_type, hash_ptr, "hash").unwrap().into_int_value();
            let c64   = self.builder.build_int_z_extend(c, i64_type, "c64").unwrap();
            let h5    = self.builder.build_left_shift(hash, i64_type.const_int(5, false), "h5").unwrap();
            let h33   = self.builder.build_int_add(h5, hash, "h33").unwrap();
            let nh    = self.builder.build_xor(h33, c64, "nh").unwrap();
            self.builder.build_store(hash_ptr, nh).unwrap();
            let np = self.builder.build_int_add(pos, i64_type.const_int(1, false), "np").unwrap();
            self.builder.build_store(pos_ptr, np).unwrap();
            self.builder.build_unconditional_branch(cond_bb).unwrap();

            self.builder.position_at_end(exit_bb);
            let hash  = self.builder.build_load(i64_type, hash_ptr, "hash").unwrap().into_int_value();
            let mask  = i64_type.const_int(cap - 1, false);
            let s64   = self.builder.build_and(hash, mask, "s64").unwrap();
            let slot  = self.builder.build_int_truncate(s64, i32_type, "slot").unwrap();
            self.builder.build_return(Some(&slot)).unwrap();
        }

        // Helper macro-like closures can't be used easily, so I'll repeat the probe pattern per function.
        // Layout for map[i32, i32]:  keys at i*4, vals at 16384+i*4, used at 32768+i*4  alloc=49152
        // Layout for map[str, i32]:  keys at i*8, vals at 32768+i*4, used at 49152+i*4  alloc=65536

        // ====== __vit_map_i32i32_set(i8*, i32 key, i32 val) -> void ======
        {
            let f = self.module.add_function("__vit_map_i32i32_set",
                void_type.fn_type(&[i8_ptr.into(), i32_type.into(), i32_type.into()], false), None);
            let entry     = self.context.append_basic_block(f, "entry");
            let lp        = self.context.append_basic_block(f, "lp");
            let chk       = self.context.append_basic_block(f, "chk");
            let ins       = self.context.append_basic_block(f, "ins");
            let nxt       = self.context.append_basic_block(f, "nxt");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let val = f.get_nth_param(2).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i32, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i32, "a").unwrap();
            let slot = self.builder.build_int_signed_rem(adj, cap_i32, "s").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, ins, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64, f4, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i32_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, ins, nxt).unwrap();

            self.builder.position_at_end(ins);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            self.builder.build_store(up, i32_type.const_int(1, false)).unwrap();
            let ko  = self.builder.build_int_mul(i64, f4, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            self.builder.build_store(kp, key).unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(16384, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            self.builder.build_store(vp, val).unwrap();
            self.builder.build_return(None).unwrap();

            self.builder.position_at_end(nxt);
            let i     = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1    = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m   = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();
        }

        // ====== __vit_map_i32i32_get(i8*, i32 key) -> i32 ======
        {
            let f = self.module.add_function("__vit_map_i32i32_get",
                i32_type.fn_type(&[i8_ptr.into(), i32_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let fnd   = self.context.append_basic_block(f, "fnd");
            let nxt   = self.context.append_basic_block(f, "nxt");
            let nf    = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map  = f.get_nth_param(0).unwrap().into_pointer_value();
            let key  = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i32, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i32, "a").unwrap();
            let slot = self.builder.build_int_signed_rem(adj, cap_i32, "s").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64, f4, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i32_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(16384, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            let v   = self.builder.build_load(i32_type, vp, "v").unwrap();
            self.builder.build_return(Some(&v)).unwrap();

            self.builder.position_at_end(nxt);
            let i     = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1    = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m   = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        // ====== __vit_map_i32i32_has(i8*, i32 key) -> i32 (1=found, 0=not) ======
        {
            let f = self.module.add_function("__vit_map_i32i32_has",
                i32_type.fn_type(&[i8_ptr.into(), i32_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let fnd   = self.context.append_basic_block(f, "fnd");
            let nxt   = self.context.append_basic_block(f, "nxt");
            let nf    = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map  = f.get_nth_param(0).unwrap().into_pointer_value();
            let key  = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i32, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i32, "a").unwrap();
            let slot = self.builder.build_int_signed_rem(adj, cap_i32, "s").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64, f4, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i32_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            self.builder.build_return(Some(&i32_type.const_int(1, false))).unwrap();

            self.builder.position_at_end(nxt);
            let i     = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1    = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m   = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        let strcmp   = self.module.get_function("strcmp").unwrap();
        let hash_str = self.module.get_function("__vit_hash_str").unwrap();

        // ====== __vit_map_stri32_set(i8*, i8* key, i32 val) -> void ======
        // keys[i] at i*8, vals at 32768+i*4, used at 49152+i*4
        {
            let f = self.module.add_function("__vit_map_stri32_set",
                void_type.fn_type(&[i8_ptr.into(), i8_ptr.into(), i32_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let ins   = self.context.append_basic_block(f, "ins");
            let nxt   = self.context.append_basic_block(f, "nxt");

            self.builder.position_at_end(entry);
            let map  = f.get_nth_param(0).unwrap().into_pointer_value();
            let key  = f.get_nth_param(1).unwrap().into_pointer_value();
            let val  = f.get_nth_param(2).unwrap().into_int_value();
            let slot = self.builder.build_call(hash_str, &[key.into()], "slot").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, ins, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f8  = i64_type.const_int(8, false);
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i8_ptr, kp, "k").unwrap().into_pointer_value();
            let cmp = self.builder.build_call(strcmp, &[k.into(), key.into()], "cmp").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, cmp, i32_type.const_int(0, false), "sm").unwrap();
            self.builder.build_conditional_branch(sm, ins, nxt).unwrap();

            self.builder.position_at_end(ins);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let f8  = i64_type.const_int(8, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            self.builder.build_store(up, i32_type.const_int(1, false)).unwrap();
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            self.builder.build_store(kp, key).unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f4, "xv").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            self.builder.build_store(vp, val).unwrap();
            self.builder.build_return(None).unwrap();

            self.builder.position_at_end(nxt);
            let i     = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1    = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m   = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();
        }

        // ====== __vit_map_stri32_get(i8*, i8* key) -> i32 ======
        {
            let f = self.module.add_function("__vit_map_stri32_get",
                i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let fnd   = self.context.append_basic_block(f, "fnd");
            let nxt   = self.context.append_basic_block(f, "nxt");
            let nf    = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map  = f.get_nth_param(0).unwrap().into_pointer_value();
            let key  = f.get_nth_param(1).unwrap().into_pointer_value();
            let slot = self.builder.build_call(hash_str, &[key.into()], "slot").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f8  = i64_type.const_int(8, false);
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i8_ptr, kp, "k").unwrap().into_pointer_value();
            let cmp = self.builder.build_call(strcmp, &[k.into(), key.into()], "cmp").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, cmp, i32_type.const_int(0, false), "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            let v   = self.builder.build_load(i32_type, vp, "v").unwrap();
            self.builder.build_return(Some(&v)).unwrap();

            self.builder.position_at_end(nxt);
            let i     = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1    = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m   = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        // ====== __vit_map_stri32_has(i8*, i8* key) -> i32 ======
        {
            let f = self.module.add_function("__vit_map_stri32_has",
                i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let fnd   = self.context.append_basic_block(f, "fnd");
            let nxt   = self.context.append_basic_block(f, "nxt");
            let nf    = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map  = f.get_nth_param(0).unwrap().into_pointer_value();
            let key  = f.get_nth_param(1).unwrap().into_pointer_value();
            let slot = self.builder.build_call(hash_str, &[key.into()], "slot").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f8  = i64_type.const_int(8, false);
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i8_ptr, kp, "k").unwrap().into_pointer_value();
            let cmp = self.builder.build_call(strcmp, &[k.into(), key.into()], "cmp").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, cmp, i32_type.const_int(0, false), "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            self.builder.build_return(Some(&i32_type.const_int(1, false))).unwrap();

            self.builder.position_at_end(nxt);
            let i     = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1    = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m   = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        // ====================================================================
        // map[str, str]: key_off=i*8, val_off=32768+i*8, used_off=65536+i*4
        //                alloc = 4096*8 + 4096*8 + 4096*4 = 81920
        // ====================================================================

        // ====== __vit_map_strstr_set(i8* map, i8* key, i8* val) -> void ======
        {
            let f = self.module.add_function("__vit_map_strstr_set",
                void_type.fn_type(&[i8_ptr.into(), i8_ptr.into(), i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let ins   = self.context.append_basic_block(f, "ins");
            let nxt   = self.context.append_basic_block(f, "nxt");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_pointer_value();
            let val = f.get_nth_param(2).unwrap().into_pointer_value();
            let slot = self.builder.build_call(hash_str, &[key.into()], "slot").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let pp = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, ins, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f8  = i64_type.const_int(8, false);
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i8_ptr, kp, "k").unwrap().into_pointer_value();
            let cmp = self.builder.build_call(strcmp, &[k.into(), key.into()], "cmp").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, cmp, i32_type.const_int(0, false), "sm").unwrap();
            self.builder.build_conditional_branch(sm, ins, nxt).unwrap();

            self.builder.position_at_end(ins);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let f8  = i64_type.const_int(8, false);
            // mark used
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64, f4, "xu").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            self.builder.build_store(up, i32_type.const_int(1, false)).unwrap();
            // store key
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            self.builder.build_store(kp, key).unwrap();
            // store val
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f8, "xv").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            self.builder.build_store(vp, val).unwrap();
            self.builder.build_return(None).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();
        }

        // ====== __vit_map_strstr_get(i8* map, i8* key) -> i8* ======
        {
            let null_str: inkwell::values::BasicValueEnum = i8_ptr.const_null().into();
            let f = self.module.add_function("__vit_map_strstr_get",
                i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let fnd   = self.context.append_basic_block(f, "fnd");
            let nxt   = self.context.append_basic_block(f, "nxt");
            let nf    = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map  = f.get_nth_param(0).unwrap().into_pointer_value();
            let key  = f.get_nth_param(1).unwrap().into_pointer_value();
            let slot = self.builder.build_call(hash_str, &[key.into()], "slot").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f8  = i64_type.const_int(8, false);
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i8_ptr, kp, "k").unwrap().into_pointer_value();
            let cmp = self.builder.build_call(strcmp, &[k.into(), key.into()], "cmp").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, cmp, i32_type.const_int(0, false), "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f8  = i64_type.const_int(8, false);
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64, f8, "xv").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            let v   = self.builder.build_load(i8_ptr, vp, "v").unwrap();
            self.builder.build_return(Some(&v)).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&null_str)).unwrap();
        }

        // ====== __vit_map_strstr_has(i8* map, i8* key) -> i32 ======
        {
            let f = self.module.add_function("__vit_map_strstr_has",
                i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp    = self.context.append_basic_block(f, "lp");
            let chk   = self.context.append_basic_block(f, "chk");
            let fnd   = self.context.append_basic_block(f, "fnd");
            let nxt   = self.context.append_basic_block(f, "nxt");
            let nf    = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map  = f.get_nth_param(0).unwrap().into_pointer_value();
            let key  = f.get_nth_param(1).unwrap().into_pointer_value();
            let slot = self.builder.build_call(hash_str, &[key.into()], "slot").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f4  = i64_type.const_int(4, false);
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64, f4, "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64 = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let f8  = i64_type.const_int(8, false);
            let ko  = self.builder.build_int_mul(i64, f8, "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i8_ptr, kp, "k").unwrap().into_pointer_value();
            let cmp = self.builder.build_call(strcmp, &[k.into(), key.into()], "cmp").unwrap()
                .try_as_basic_value().left().unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, cmp, i32_type.const_int(0, false), "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            self.builder.build_return(Some(&i32_type.const_int(1, false))).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        // ====================================================================
        // map[i32, i64]: key_off=i*4, val_off=16384+i*8, used_off=49152+i*4
        // ====================================================================

        // --- __vit_map_i32i64_set ---
        {
            let f = self.module.add_function("__vit_map_i32i64_set",
                void_type.fn_type(&[i8_ptr.into(), i32_type.into(), i64_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let ins = self.context.append_basic_block(f, "ins");
            let nxt = self.context.append_basic_block(f, "nxt");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value(); // i32
            let val = f.get_nth_param(2).unwrap().into_int_value(); // i64
            let rem  = self.builder.build_int_signed_rem(key, cap_i32, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i32, "a").unwrap();
            let slot = self.builder.build_int_signed_rem(adj, cap_i32, "s").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, ins, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i32_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, ins, nxt).unwrap();

            self.builder.position_at_end(ins);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            self.builder.build_store(up, i32_type.const_int(1, false)).unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            self.builder.build_store(kp, key).unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(16384, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "xv").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            self.builder.build_store(vp, val).unwrap();
            self.builder.build_return(None).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();
        }

        // --- __vit_map_i32i64_get ---
        {
            let f = self.module.add_function("__vit_map_i32i64_get",
                i64_type.fn_type(&[i8_ptr.into(), i32_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let fnd = self.context.append_basic_block(f, "fnd");
            let nxt = self.context.append_basic_block(f, "nxt");
            let nf  = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i32, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i32, "a").unwrap();
            let slot = self.builder.build_int_signed_rem(adj, cap_i32, "s").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i32_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(16384, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "x").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            let v   = self.builder.build_load(i64_type, vp, "v").unwrap();
            self.builder.build_return(Some(&v)).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i64_type.const_int(0, false))).unwrap();
        }

        // --- __vit_map_i32i64_has ---
        {
            let f = self.module.add_function("__vit_map_i32i64_has",
                i32_type.fn_type(&[i8_ptr.into(), i32_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let fnd = self.context.append_basic_block(f, "fnd");
            let nxt = self.context.append_basic_block(f, "nxt");
            let nf  = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i32, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i32, "a").unwrap();
            let slot = self.builder.build_int_signed_rem(adj, cap_i32, "s").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i32_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            self.builder.build_return(Some(&i32_type.const_int(1, false))).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        // ====================================================================
        // map[i64, i32]: key_off=i*8, val_off=32768+i*4, used_off=49152+i*4
        // ====================================================================
        let cap_i64 = i64_type.const_int(cap, false);

        // --- __vit_map_i64i32_set ---
        {
            let f = self.module.add_function("__vit_map_i64i32_set",
                void_type.fn_type(&[i8_ptr.into(), i64_type.into(), i32_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let ins = self.context.append_basic_block(f, "ins");
            let nxt = self.context.append_basic_block(f, "nxt");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value(); // i64
            let val = f.get_nth_param(2).unwrap().into_int_value(); // i32
            let rem  = self.builder.build_int_signed_rem(key, cap_i64, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i64, "a").unwrap();
            let slot64 = self.builder.build_int_signed_rem(adj, cap_i64, "s").unwrap();
            let slot = self.builder.build_int_truncate(slot64, i32_type, "slot").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, ins, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i64_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, ins, nxt).unwrap();

            self.builder.position_at_end(ins);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            self.builder.build_store(up, i32_type.const_int(1, false)).unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            self.builder.build_store(kp, key).unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "xv").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            self.builder.build_store(vp, val).unwrap();
            self.builder.build_return(None).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();
        }

        // --- __vit_map_i64i32_get ---
        {
            let f = self.module.add_function("__vit_map_i64i32_get",
                i32_type.fn_type(&[i8_ptr.into(), i64_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let fnd = self.context.append_basic_block(f, "fnd");
            let nxt = self.context.append_basic_block(f, "nxt");
            let nf  = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i64, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i64, "a").unwrap();
            let slot64 = self.builder.build_int_signed_rem(adj, cap_i64, "s").unwrap();
            let slot = self.builder.build_int_truncate(slot64, i32_type, "slot").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i64_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            let v   = self.builder.build_load(i32_type, vp, "v").unwrap();
            self.builder.build_return(Some(&v)).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        // --- __vit_map_i64i32_has ---
        {
            let f = self.module.add_function("__vit_map_i64i32_has",
                i32_type.fn_type(&[i8_ptr.into(), i64_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let fnd = self.context.append_basic_block(f, "fnd");
            let nxt = self.context.append_basic_block(f, "nxt");
            let nf  = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i64, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i64, "a").unwrap();
            let slot64 = self.builder.build_int_signed_rem(adj, cap_i64, "s").unwrap();
            let slot = self.builder.build_int_truncate(slot64, i32_type, "slot").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(49152, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i64_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            self.builder.build_return(Some(&i32_type.const_int(1, false))).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }

        // ====================================================================
        // map[i64, i64]: key_off=i*8, val_off=32768+i*8, used_off=65536+i*4
        // ====================================================================

        // --- __vit_map_i64i64_set ---
        {
            let f = self.module.add_function("__vit_map_i64i64_set",
                void_type.fn_type(&[i8_ptr.into(), i64_type.into(), i64_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let ins = self.context.append_basic_block(f, "ins");
            let nxt = self.context.append_basic_block(f, "nxt");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let val = f.get_nth_param(2).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i64, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i64, "a").unwrap();
            let slot64 = self.builder.build_int_signed_rem(adj, cap_i64, "s").unwrap();
            let slot = self.builder.build_int_truncate(slot64, i32_type, "slot").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, ins, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i64_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, ins, nxt).unwrap();

            self.builder.position_at_end(ins);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            self.builder.build_store(up, i32_type.const_int(1, false)).unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            self.builder.build_store(kp, key).unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "xv").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            self.builder.build_store(vp, val).unwrap();
            self.builder.build_return(None).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();
        }

        // --- __vit_map_i64i64_get ---
        {
            let f = self.module.add_function("__vit_map_i64i64_get",
                i64_type.fn_type(&[i8_ptr.into(), i64_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let fnd = self.context.append_basic_block(f, "fnd");
            let nxt = self.context.append_basic_block(f, "nxt");
            let nf  = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i64, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i64, "a").unwrap();
            let slot64 = self.builder.build_int_signed_rem(adj, cap_i64, "s").unwrap();
            let slot = self.builder.build_int_truncate(slot64, i32_type, "slot").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i64_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let vo  = self.builder.build_int_add(i64_type.const_int(32768, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "x").unwrap(), "vo").unwrap();
            let vp  = unsafe { self.builder.build_gep(i8_type, map, &[vo], "vp") }.unwrap();
            let v   = self.builder.build_load(i64_type, vp, "v").unwrap();
            self.builder.build_return(Some(&v)).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i64_type.const_int(0, false))).unwrap();
        }

        // --- __vit_map_i64i64_has ---
        {
            let f = self.module.add_function("__vit_map_i64i64_has",
                i32_type.fn_type(&[i8_ptr.into(), i64_type.into()], false), None);
            let entry = self.context.append_basic_block(f, "entry");
            let lp  = self.context.append_basic_block(f, "lp");
            let chk = self.context.append_basic_block(f, "chk");
            let fnd = self.context.append_basic_block(f, "fnd");
            let nxt = self.context.append_basic_block(f, "nxt");
            let nf  = self.context.append_basic_block(f, "nf");

            self.builder.position_at_end(entry);
            let map = f.get_nth_param(0).unwrap().into_pointer_value();
            let key = f.get_nth_param(1).unwrap().into_int_value();
            let rem  = self.builder.build_int_signed_rem(key, cap_i64, "r").unwrap();
            let adj  = self.builder.build_int_add(rem, cap_i64, "a").unwrap();
            let slot64 = self.builder.build_int_signed_rem(adj, cap_i64, "s").unwrap();
            let slot = self.builder.build_int_truncate(slot64, i32_type, "slot").unwrap();
            let pp   = self.builder.build_alloca(i32_type, "pp").unwrap();
            self.builder.build_store(pp, slot).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(lp);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let uo  = self.builder.build_int_add(i64_type.const_int(65536, false),
                        self.builder.build_int_mul(i64v, i64_type.const_int(4, false), "x").unwrap(), "uo").unwrap();
            let up  = unsafe { self.builder.build_gep(i8_type, map, &[uo], "up") }.unwrap();
            let u   = self.builder.build_load(i32_type, up, "u").unwrap().into_int_value();
            let ie  = self.builder.build_int_compare(IntPredicate::EQ, u, i32_type.const_int(0, false), "ie").unwrap();
            self.builder.build_conditional_branch(ie, nf, chk).unwrap();

            self.builder.position_at_end(chk);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i64v = self.builder.build_int_s_extend(i, i64_type, "i64").unwrap();
            let ko  = self.builder.build_int_mul(i64v, i64_type.const_int(8, false), "ko").unwrap();
            let kp  = unsafe { self.builder.build_gep(i8_type, map, &[ko], "kp") }.unwrap();
            let k   = self.builder.build_load(i64_type, kp, "k").unwrap().into_int_value();
            let sm  = self.builder.build_int_compare(IntPredicate::EQ, k, key, "sm").unwrap();
            self.builder.build_conditional_branch(sm, fnd, nxt).unwrap();

            self.builder.position_at_end(fnd);
            self.builder.build_return(Some(&i32_type.const_int(1, false))).unwrap();

            self.builder.position_at_end(nxt);
            let i   = self.builder.build_load(i32_type, pp, "i").unwrap().into_int_value();
            let i1  = self.builder.build_int_add(i, i32_type.const_int(1, false), "i1").unwrap();
            let i1m = self.builder.build_int_signed_rem(i1, cap_i32, "i1m").unwrap();
            self.builder.build_store(pp, i1m).unwrap();
            self.builder.build_unconditional_branch(lp).unwrap();

            self.builder.position_at_end(nf);
            self.builder.build_return(Some(&i32_type.const_int(0, false))).unwrap();
        }
    }

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

    fn declare_extern_functions(&mut self, externs: &[ExternFunction]) {
        let i8_ptr = self.context.i8_type().ptr_type(AddressSpace::default());

        for ext in externs {
            // Skip if already declared by builtins (e.g. strlen, malloc, socket…)
            if self.module.get_function(&ext.name).is_some() {
                continue;
            }

            // Build param types — arrays passed as pointer to element
            let param_types: Vec<BasicMetadataTypeEnum> = ext.parameters.iter().map(|p| {
                match &p.typ {
                    Type::Array { element, .. } => {
                        let elem = self.convert_type(element);
                        match elem {
                            BasicTypeEnum::IntType(t)     => t.ptr_type(AddressSpace::default()).into(),
                            BasicTypeEnum::FloatType(t)   => t.ptr_type(AddressSpace::default()).into(),
                            BasicTypeEnum::PointerType(t) => t.ptr_type(AddressSpace::default()).into(),
                            _ => i8_ptr.into(),
                        }
                    }
                    _ => self.convert_type(&p.typ).into(),
                }
            }).collect();

            // Void vs non-void return type
            let fn_type = if let Type::Void = &ext.return_type {
                self.context.void_type().fn_type(&param_types, false)
            } else {
                let ret = self.convert_type(&ext.return_type);
                self.build_fn_type(ret, &param_types)
            };

            self.module.add_function(&ext.name, fn_type, None);
        }
    }

    fn declare_net_builtins(&mut self) {
        let i8_ptr  = self.context.i8_type().ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let void_type = self.context.void_type();

        // Only declare if not already declared (e.g. via extern fn in source)
        macro_rules! decl {
            ($name:expr, $ty:expr) => {
                if self.module.get_function($name).is_none() {
                    self.module.add_function($name, $ty, None);
                }
            };
        }
        decl!("socket",     i32_type.fn_type(&[i32_type.into(), i32_type.into(), i32_type.into()], false));
        decl!("setsockopt", i32_type.fn_type(&[i32_type.into(), i32_type.into(), i32_type.into(), i8_ptr.into(), i32_type.into()], false));
        decl!("bind",       i32_type.fn_type(&[i32_type.into(), i8_ptr.into(), i32_type.into()], false));
        decl!("listen",     i32_type.fn_type(&[i32_type.into(), i32_type.into()], false));
        decl!("accept",     i32_type.fn_type(&[i32_type.into(), i8_ptr.into(), i8_ptr.into()], false));
        decl!("recv",       i32_type.fn_type(&[i32_type.into(), i8_ptr.into(), i32_type.into(), i32_type.into()], false));
        decl!("send",       i32_type.fn_type(&[i32_type.into(), i8_ptr.into(), i32_type.into(), i32_type.into()], false));
        decl!("close",      i32_type.fn_type(&[i32_type.into()], false));
        decl!("fork",       i32_type.fn_type(&[], false));
        decl!("signal",     i8_ptr.fn_type(&[i32_type.into(), i8_ptr.into()], false));
        decl!("_exit",      void_type.fn_type(&[i32_type.into()], false));
    }

    fn build_tcp_helpers(&mut self) {
        let i8_type  = self.context.i8_type();
        let i8_ptr   = i8_type.ptr_type(AddressSpace::default());
        let i16_type = self.context.i16_type();
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();

        // ── __vit_tcp_listen(port: i32) -> i32 ──────────────────────────────
        // socket() + setsockopt(SO_REUSEADDR) + bind() + listen()
        {
            let f = self.module.add_function("__vit_tcp_listen",
                i32_type.fn_type(&[i32_type.into()], false), None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);

            let port = f.get_nth_param(0).unwrap().into_int_value();

            // fd = socket(AF_INET=2, SOCK_STREAM=1, 0)
            let fd = self.builder.build_call(self.module.get_function("socket").unwrap(), &[
                i32_type.const_int(2, false).into(),
                i32_type.const_int(1, false).into(),
                i32_type.const_int(0, false).into(),
            ], "fd").unwrap().try_as_basic_value().left().unwrap().into_int_value();

            // int opt = 1; setsockopt(fd, SOL_SOCKET=1, SO_REUSEADDR=2, &opt, 4)
            let opt = self.builder.build_alloca(i32_type, "opt").unwrap();
            self.builder.build_store(opt, i32_type.const_int(1, false)).unwrap();
            self.builder.build_call(self.module.get_function("setsockopt").unwrap(), &[
                fd.into(),
                i32_type.const_int(1, false).into(),
                i32_type.const_int(2, false).into(),
                opt.into(),
                i32_type.const_int(4, false).into(),
            ], "").unwrap();

            // char addr[16] = {0}  (sockaddr_in)
            let arr16 = i8_type.array_type(16);
            let addr  = self.builder.build_alloca(arr16, "addr").unwrap();
            self.builder.build_store(addr, arr16.const_zero()).unwrap();

            // GEP to byte 0 of addr
            let zero32 = i32_type.const_int(0, false);
            let base = unsafe {
                self.builder.build_gep(arr16, addr, &[zero32, zero32], "base")
            }.unwrap();

            // sin_family = AF_INET (2) as i16 at offset 0
            self.builder.build_store(base, i16_type.const_int(2, false)).unwrap();

            // sin_port = htons(port) as i16 at offset 2
            let port16  = self.builder.build_int_truncate(port, i16_type, "p16").unwrap();
            let lo      = self.builder.build_and(port16, i16_type.const_int(0xFF, false), "lo").unwrap();
            let hi      = self.builder.build_right_shift(port16, i16_type.const_int(8, false), false, "hi").unwrap();
            let lo_sh   = self.builder.build_left_shift(lo, i16_type.const_int(8, false), "lsh").unwrap();
            let htons   = self.builder.build_or(lo_sh, hi, "htons").unwrap();
            let pptr    = unsafe {
                self.builder.build_gep(i8_type, base, &[i64_type.const_int(2, false)], "pptr")
            }.unwrap();
            self.builder.build_store(pptr, htons).unwrap();

            // bind(fd, &addr, 16)
            self.builder.build_call(self.module.get_function("bind").unwrap(), &[
                fd.into(), base.into(), i32_type.const_int(16, false).into(),
            ], "").unwrap();

            // listen(fd, 128)
            self.builder.build_call(self.module.get_function("listen").unwrap(), &[
                fd.into(), i32_type.const_int(128, false).into(),
            ], "").unwrap();

            self.builder.build_return(Some(&fd)).unwrap();
        }

        // ── __vit_tcp_accept(fd: i32) -> i32 ────────────────────────────────
        {
            let f = self.module.add_function("__vit_tcp_accept",
                i32_type.fn_type(&[i32_type.into()], false), None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);

            let fd     = f.get_nth_param(0).unwrap().into_int_value();
            let null   = i8_ptr.const_null();
            let client = self.builder.build_call(self.module.get_function("accept").unwrap(), &[
                fd.into(), null.into(), null.into(),
            ], "client").unwrap().try_as_basic_value().left().unwrap().into_int_value();
            self.builder.build_return(Some(&client)).unwrap();
        }

        // ── __vit_tcp_recv(fd: i32, buf: i8*, size: i32) -> i32 ─────────────
        {
            let f = self.module.add_function("__vit_tcp_recv",
                i32_type.fn_type(&[i32_type.into(), i8_ptr.into(), i32_type.into()], false), None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);

            let fd   = f.get_nth_param(0).unwrap().into_int_value();
            let buf  = f.get_nth_param(1).unwrap().into_pointer_value();
            let size = f.get_nth_param(2).unwrap().into_int_value();
            let n    = self.builder.build_call(self.module.get_function("recv").unwrap(), &[
                fd.into(), buf.into(), size.into(), i32_type.const_int(0, false).into(),
            ], "n").unwrap().try_as_basic_value().left().unwrap().into_int_value();
            self.builder.build_return(Some(&n)).unwrap();
        }

        // ── __vit_tcp_send(fd: i32, buf: i8*, len: i32) -> i32 ──────────────
        {
            let f = self.module.add_function("__vit_tcp_send",
                i32_type.fn_type(&[i32_type.into(), i8_ptr.into(), i32_type.into()], false), None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);

            let fd  = f.get_nth_param(0).unwrap().into_int_value();
            let buf = f.get_nth_param(1).unwrap().into_pointer_value();
            let len = f.get_nth_param(2).unwrap().into_int_value();
            let n   = self.builder.build_call(self.module.get_function("send").unwrap(), &[
                fd.into(), buf.into(), len.into(), i32_type.const_int(0, false).into(),
            ], "n").unwrap().try_as_basic_value().left().unwrap().into_int_value();
            self.builder.build_return(Some(&n)).unwrap();
        }

        // ── __vit_tcp_close(fd: i32) -> i32 ─────────────────────────────────
        {
            let f = self.module.add_function("__vit_tcp_close",
                i32_type.fn_type(&[i32_type.into()], false), None);
            let bb = self.context.append_basic_block(f, "entry");
            self.builder.position_at_end(bb);

            let fd = f.get_nth_param(0).unwrap().into_int_value();
            let r  = self.builder.build_call(self.module.get_function("close").unwrap(), &[
                fd.into(),
            ], "r").unwrap().try_as_basic_value().left().unwrap().into_int_value();
            self.builder.build_return(Some(&r)).unwrap();
        }
    }

    fn generate_struct_defs(&mut self, structs: &[StructDef]) {
        // Pass 1: register opaque named struct types for all user structs.
        // This allows forward references (struct A with field of type B, and vice-versa).
        for s in structs {
            let opaque = self.context.opaque_struct_type(&s.name);
            let field_names: Vec<String> = s.fields.iter().map(|f| f.name.clone()).collect();
            self.struct_defs.insert(s.name.clone(), (opaque, field_names));
            self.struct_field_types.insert(
                s.name.clone(),
                s.fields.iter().map(|f| f.typ.clone()).collect(),
            );
        }
        // Pass 2: fill in each struct body — convert_type can now resolve nested struct types.
        for s in structs {
            let field_types: Vec<BasicTypeEnum> = s.fields.iter()
                .map(|f| self.convert_type(&f.typ))
                .collect();
            let (opaque, _) = self.struct_defs[&s.name].clone();
            opaque.set_body(&field_types, false);
        }
    }

    /// Declares the global route table used by http_handle / http_listen.
    fn declare_http_route_table(&mut self) {
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());
        let arr64    = ptr_type.array_type(64);

        let g = self.module.add_global(i32_type, None, "__vit_route_count");
        g.set_initializer(&i32_type.const_int(0, false));

        let gm = self.module.add_global(arr64, None, "__vit_route_methods");
        gm.set_initializer(&arr64.const_zero());

        let gp = self.module.add_global(arr64, None, "__vit_route_paths");
        gp.set_initializer(&arr64.const_zero());

        let gh = self.module.add_global(arr64, None, "__vit_route_handlers");
        gh.set_initializer(&arr64.const_zero());
    }

    fn ensure_http_handler_wrapper(&mut self, handler_name: &str) -> Result<PointerValue<'ctx>, String> {
        let i8_ptr = self.context.i8_type().ptr_type(AddressSpace::default());
        let wrapper_name = format!("__vit_http_wrap_{}", handler_name);
        let saved_bb = self.builder.get_insert_block();

        if let Some(existing) = self.module.get_function(&wrapper_name) {
            return Ok(existing.as_global_value().as_pointer_value());
        }

        let handler_fn = self.module.get_function(handler_name)
            .ok_or_else(|| format!("http_handle(): unknown function '{}'", handler_name))?;
        let ret = handler_fn.get_type().get_return_type()
            .ok_or_else(|| format!("http_handle(): handler '{}' must return str or Response", handler_name))?;

        match ret {
            BasicTypeEnum::PointerType(_) => {
                Ok(handler_fn.as_global_value().as_pointer_value())
            }
            BasicTypeEnum::StructType(ret_struct) => {
                let (response_type, _) = self.struct_defs.get("Response")
                    .ok_or_else(|| "http_handle(): Response struct not found — did you import lib/http.vit?".to_string())?
                    .clone();
                if ret_struct != response_type {
                    return Err(format!(
                        "http_handle(): handler '{}' must return str or Response",
                        handler_name
                    ));
                }

                let http_build_fn = self.module.get_function("http_build")
                    .ok_or_else(|| "http_handle(): http_build not found — did you import lib/http.vit?".to_string())?;
                let http_response_free_fn = self.module.get_function("http_response_free")
                    .ok_or_else(|| "http_handle(): http_response_free not found — did you import lib/http.vit?".to_string())?;
                let wrapper = self.module.add_function(
                    &wrapper_name,
                    i8_ptr.fn_type(&[i8_ptr.into()], false),
                    None,
                );
                let entry = self.context.append_basic_block(wrapper, "entry");
                self.builder.position_at_end(entry);

                let req_ptr = wrapper.get_nth_param(0).unwrap().into_pointer_value();
                let resp = self.builder
                    .build_call(handler_fn, &[req_ptr.into()], "handler_resp")
                    .unwrap()
                    .try_as_basic_value()
                    .left()
                    .ok_or_else(|| format!("http_handle(): handler '{}' returned no value", handler_name))?;
                let built = self.builder
                    .build_call(http_build_fn, &[resp.into()], "built_resp")
                    .unwrap()
                    .try_as_basic_value()
                    .left()
                    .ok_or_else(|| "http_handle(): http_build returned no value".to_string())?;
                self.builder.build_call(http_response_free_fn, &[resp.into()], "free_resp").unwrap();
                self.builder.build_return(Some(&built)).unwrap();
                if let Some(bb) = saved_bb {
                    self.builder.position_at_end(bb);
                }

                Ok(wrapper.as_global_value().as_pointer_value())
            }
            _ => Err(format!(
                "http_handle(): handler '{}' must return str or Response",
                handler_name
            )),
        }
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
                    // Struct param: incoming value is a pointer to the struct — copy into local alloca
                    let (st, _) = self.struct_defs.get(sname)
                        .unwrap_or_else(|| panic!("Unknown struct '{}'", sname))
                        .clone();
                    let alloca = self.builder.build_alloca(st, &param.name).unwrap();
                    let src_ptr = param_value.into_pointer_value();
                    let struct_val = self.builder.build_load(st, src_ptr, "param_struct").unwrap();
                    self.builder.build_store(alloca, struct_val).unwrap();
                    self.variables.insert(param.name.clone(), (alloca, st.into()));
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

    fn generate_statement(&mut self, stmt: &Statement) -> Result<(), String> {
        match stmt {
            Statement::VariableDecl { name, typ, initializer } => {
                // Map variables: calloc backing store, track key/val types
                if let Type::Map { key, value } = typ {
                    let alloc_size = match (key.as_ref(), value.as_ref()) {
                        (Type::I32, Type::I32) => 49152u64,  // CAP*4 + CAP*4 + CAP*4
                        (Type::Str, Type::I32) => 65536u64,  // CAP*8 + CAP*4 + CAP*4
                        (Type::I32, Type::I64) => 65536u64,  // CAP*4 + CAP*8 + CAP*4
                        (Type::I64, Type::I32) => 65536u64,  // CAP*8 + CAP*4 + CAP*4
                        (Type::I64, Type::I64) => 81920u64,  // CAP*8 + CAP*8 + CAP*4
                        (Type::Str, Type::Str) => 81920u64,  // CAP*8 + CAP*8 + CAP*4
                        _ => return Err(format!("Unsupported map type: map[{}, {}]", key, value)),
                    };
                    let i8_ptr    = self.context.i8_type().ptr_type(AddressSpace::default());
                    let alloca    = self.builder.build_alloca(i8_ptr, name).unwrap();
                    let calloc_fn = self.module.get_function("calloc").unwrap();
                    let i64_type  = self.context.i64_type();
                    let map_mem   = self.builder.build_call(
                        calloc_fn,
                        &[i64_type.const_int(1, false).into(), i64_type.const_int(alloc_size, false).into()],
                        "map_alloc",
                    ).unwrap().try_as_basic_value().left().unwrap();
                    self.builder.build_store(alloca, map_mem).unwrap();
                    self.variables.insert(name.clone(), (alloca, i8_ptr.into()));
                    self.map_variables.insert(name.clone(), (*key.clone(), *value.clone()));
                    return Ok(());
                }
                let llvm_type = self.convert_type(typ);
                let alloca = self.builder.build_alloca(llvm_type, name).unwrap();

                if let Some(init) = initializer {
                    if let (BasicTypeEnum::ArrayType(at), Expression::ArrayLiteral(elems)) = (llvm_type, init) {
                        let elems = elems.clone();
                        let zero = self.context.i32_type().const_int(0, false);
                        for (i, elem) in elems.iter().enumerate() {
                            let val = self.generate_expression(elem)?;
                            let idx = self.context.i32_type().const_int(i as u64, false);
                            let elem_ptr = unsafe {
                                self.builder.build_gep(at, alloca, &[zero, idx], "arr_init")
                            }.unwrap();
                            self.builder.build_store(elem_ptr, val).unwrap();
                        }
                    } else {
                        let value = self.generate_expression(init)?;
                        let value = self.coerce_int(value, llvm_type);
                        self.builder.build_store(alloca, value).unwrap();
                    }
                }

                self.variables.insert(name.clone(), (alloca, llvm_type));
                if let Type::Struct(sname) = typ {
                    self.var_struct_names.insert(name.clone(), sname.clone());
                }
            }
            Statement::Assign { name, value } => {
                let (ptr, stored_type) = *self.variables.get(name)
                    .ok_or_else(|| format!("Undefined variable: {}", name))?;
                let new_value = self.generate_expression(value)?;
                let new_value = self.coerce_int(new_value, stored_type);
                self.builder.build_store(ptr, new_value).unwrap();
            }
            Statement::Return { value } => {
                if let Some(val) = value {
                    let return_value = self.generate_expression(val)?;
                    self.builder.build_return(Some(&return_value)).unwrap();
                } else {
                    self.builder.build_return(None).unwrap();
                }
            }
            Statement::If { condition, then_body, else_body } => {
                self.generate_if_statement(condition, then_body, else_body)?;
            }
            Statement::While { condition, body } => {
                self.generate_while_statement(condition, body)?;
            }
            Statement::Print { values } => {
                self.generate_print(values)?;
            }
            Statement::Input { name, typ } => {
                self.generate_input(name, typ)?;
            }
            Statement::InputIndex { name, index } => {
                let scanf_fn = self.scanf.unwrap();
                // Array parameter (pointer-based)
                if let Some(elem_type) = self.array_params.get(name).copied() {
                    let (ptr_alloca, ptr_t) = self.variables.get(name)
                        .copied()
                        .ok_or_else(|| format!("Undefined variable: {}", name))?;
                    let ptr = self.builder.build_load(ptr_t, ptr_alloca, "arrptr")
                        .unwrap().into_pointer_value();
                    let idx = self.generate_expression(index)?.into_int_value();
                    let elem_ptr = unsafe {
                        self.builder.build_gep(elem_type, ptr, &[idx], "input_gep")
                    }.unwrap();
                    let fmt_str = match elem_type {
                        BasicTypeEnum::IntType(t) if t.get_bit_width() == 64 => "%ld",
                        BasicTypeEnum::FloatType(t) if t == self.context.f64_type() => "%lf",
                        BasicTypeEnum::FloatType(_) => "%f",
                        _ => "%d",
                    };
                    let fmt = self.builder.build_global_string_ptr(fmt_str, "fmt_scan_idx").unwrap();
                    self.builder.build_call(scanf_fn, &[fmt.as_pointer_value().into(), elem_ptr.into()], "scanf_idx").unwrap();
                    return Ok(());
                }
                // Local array
                let (array_ptr, array_type) = self.variables.get(name)
                    .copied()
                    .ok_or_else(|| format!("Undefined variable: {}", name))?;
                let at = match array_type {
                    BasicTypeEnum::ArrayType(at) => at,
                    _ => return Err(format!("'{}' is not an array", name)),
                };
                let idx = self.generate_expression(index)?.into_int_value();
                let zero = self.context.i32_type().const_int(0, false);
                let elem_ptr = unsafe {
                    self.builder.build_gep(at, array_ptr, &[zero, idx], "input_gep")
                }.unwrap();

                let fmt_str = match at.get_element_type() {
                    BasicTypeEnum::IntType(t) if t.get_bit_width() == 64 => "%ld",
                    BasicTypeEnum::FloatType(t) if t == self.context.f64_type() => "%lf",
                    BasicTypeEnum::FloatType(_) => "%f",
                    _ => "%d",
                };
                let fmt = self.builder.build_global_string_ptr(fmt_str, "fmt_scan_idx").unwrap();
                self.builder.build_call(scanf_fn, &[fmt.as_pointer_value().into(), elem_ptr.into()], "scanf_idx").unwrap();
            }
            Statement::For { var, start, end, body } => {
                let i32_type = self.context.i32_type();
                let alloca = self.builder.build_alloca(i32_type, var).unwrap();
                let start_val = self.generate_expression(start)?.into_int_value();
                self.builder.build_store(alloca, start_val).unwrap();
                self.variables.insert(var.clone(), (alloca, i32_type.into()));

                let current_fn = self.current_function.unwrap();
                let cond_block  = self.context.append_basic_block(current_fn, "for_cond");
                let loop_block  = self.context.append_basic_block(current_fn, "for_loop");
                let inc_block   = self.context.append_basic_block(current_fn, "for_inc");
                let after_block = self.context.append_basic_block(current_fn, "for_after");

                self.builder.build_unconditional_branch(cond_block).unwrap();

                // Condition: i < end
                self.builder.position_at_end(cond_block);
                let i_val   = self.builder.build_load(i32_type, alloca, var).unwrap().into_int_value();
                let end_val = self.generate_expression(end)?.into_int_value();
                let cond    = self.builder.build_int_compare(IntPredicate::SLT, i_val, end_val, "for_cmp").unwrap();
                self.builder.build_conditional_branch(cond, loop_block, after_block).unwrap();

                // Body  (continue → inc_block, break → after_block)
                self.builder.position_at_end(loop_block);
                self.loop_stack.push((inc_block, after_block));
                for stmt in body {
                    self.generate_statement(stmt)?;
                    if self.block_terminated() { break; }
                }
                self.loop_stack.pop();
                if !self.block_terminated() {
                    self.builder.build_unconditional_branch(inc_block).unwrap();
                }

                // Increment
                self.builder.position_at_end(inc_block);
                let i_val = self.builder.build_load(i32_type, alloca, var).unwrap().into_int_value();
                let next  = self.builder.build_int_add(i_val, i32_type.const_int(1, false), "for_inc").unwrap();
                self.builder.build_store(alloca, next).unwrap();
                self.builder.build_unconditional_branch(cond_block).unwrap();

                self.builder.position_at_end(after_block);
            }
            Statement::Expr(expr) => {
                self.generate_expression(expr)?; // result discarded
            }
            Statement::Break => {
                let (_, after) = *self.loop_stack.last()
                    .ok_or("'break' outside of loop")?;
                self.builder.build_unconditional_branch(after).unwrap();
            }
            Statement::Continue => {
                let (cont, _) = *self.loop_stack.last()
                    .ok_or("'continue' outside of loop")?;
                self.builder.build_unconditional_branch(cont).unwrap();
            }
            Statement::IndexAssign { name, index, value } => {
                // Array parameter (pointer-based)
                if let Some(elem_type) = self.array_params.get(name).copied() {
                    let (ptr_alloca, ptr_t) = self.variables.get(name)
                        .copied()
                        .ok_or_else(|| format!("Undefined variable: {}", name))?;
                    let ptr = self.builder.build_load(ptr_t, ptr_alloca, "arrptr")
                        .unwrap().into_pointer_value();
                    let idx = self.generate_expression(index)?.into_int_value();
                    let elem_ptr = unsafe {
                        self.builder.build_gep(elem_type, ptr, &[idx], "gep")
                    }.unwrap();
                    let val = self.generate_expression(value)?;
                    self.builder.build_store(elem_ptr, val).unwrap();
                    return Ok(());
                }
                // Local array
                let (array_ptr, array_type) = self.variables.get(name)
                    .copied()
                    .ok_or_else(|| format!("Undefined variable: {}", name))?;
                let at = match array_type {
                    BasicTypeEnum::ArrayType(at) => at,
                    _ => return Err(format!("'{}' is not an array", name)),
                };
                let idx = self.generate_expression(index)?.into_int_value();
                let zero = self.context.i32_type().const_int(0, false);
                let elem_ptr = unsafe {
                    self.builder.build_gep(at, array_ptr, &[zero, idx], "gep")
                }.unwrap();
                let val = self.generate_expression(value)?;
                self.builder.build_store(elem_ptr, val).unwrap();
            }
            Statement::FieldAssign { path, value } => {
                // path = ["var", "field1", ..., "fieldN"]
                // Navigate all but the last element as nested struct ptrs,
                // then GEP to the final field for the store.
                if path.len() < 2 {
                    return Err("FieldAssign path must have at least two segments".to_string());
                }
                let (target_field) = path.last().unwrap().clone();
                let struct_path = &path[..path.len() - 1];

                // Build an Expression chain to resolve the struct pointer
                let root_expr = Expression::Identifier(struct_path[0].clone());
                let obj_expr = struct_path[1..].iter().fold(root_expr, |acc, seg| {
                    Expression::FieldAccess {
                        object: Box::new(acc),
                        field: seg.clone(),
                    }
                });

                let (struct_ptr, struct_name) = self.resolve_struct_ptr(&obj_expr)?;
                let (st, field_names) = self.struct_defs.get(&struct_name)
                    .ok_or_else(|| format!("Unknown struct '{}'", struct_name))?
                    .clone();
                let idx = field_names.iter().position(|n| n == &target_field)
                    .ok_or_else(|| format!("Struct '{}' has no field '{}'", struct_name, target_field))? as u32;
                let field_ptr = self.builder.build_struct_gep(st, struct_ptr, idx, "fptr").unwrap();
                let val = self.generate_expression(value)?;
                self.builder.build_store(field_ptr, val).unwrap();
            }
        }

        Ok(())
    }

    /// Converts any integer value to i1 (bool) by comparing != 0.
    /// Required because LLVM branch instructions demand i1, not i32.
    fn to_i1(&self, val: inkwell::values::IntValue<'ctx>, name: &str) -> inkwell::values::IntValue<'ctx> {
        let ty = val.get_type();
        if ty == self.context.bool_type() {
            return val;
        }
        self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            val,
            ty.const_int(0, false),
            name,
        ).unwrap()
    }

    fn generate_while_statement(
        &mut self,
        condition: &Expression,
        body: &[Statement],
    ) -> Result<(), String> {
        let current_fn = self.current_function.unwrap();
        let cond_block  = self.context.append_basic_block(current_fn, "while_cond");
        let loop_block  = self.context.append_basic_block(current_fn, "while_loop");
        let after_block = self.context.append_basic_block(current_fn, "while_after");

        self.builder.build_unconditional_branch(cond_block).unwrap();

        self.builder.position_at_end(cond_block);
        let cond_value = self.generate_expression(condition)?;
        let cond_i1 = self.to_i1(cond_value.into_int_value(), "while_cond");
        self.builder.build_conditional_branch(cond_i1, loop_block, after_block).unwrap();

        self.builder.position_at_end(loop_block);
        self.loop_stack.push((cond_block, after_block));
        for stmt in body {
            self.generate_statement(stmt)?;
            if self.block_terminated() { break; }
        }
        self.loop_stack.pop();
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(cond_block).unwrap();
        }

        self.builder.position_at_end(after_block);
        Ok(())
    }

    fn generate_if_statement(
        &mut self,
        condition: &Expression,
        then_body: &[Statement],
        else_body: &Option<Vec<Statement>>,
    ) -> Result<(), String> {
        let cond_value = self.generate_expression(condition)?;
        let cond_i1    = self.to_i1(cond_value.into_int_value(), "if_cond");

        let current_fn = self.current_function.unwrap();
        let then_block = self.context.append_basic_block(current_fn, "then");
        let else_block = self.context.append_basic_block(current_fn, "else");
        let merge_block = self.context.append_basic_block(current_fn, "merge");

        self.builder.build_conditional_branch(
            cond_i1,
            then_block,
            else_block,
        ).unwrap();

        // Then block
        self.builder.position_at_end(then_block);
        for stmt in then_body {
            self.generate_statement(stmt)?;
            if self.block_terminated() { break; }
        }
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(merge_block).unwrap();
        }

        // Else block
        self.builder.position_at_end(else_block);
        if let Some(else_b) = else_body {
            for stmt in else_b {
                self.generate_statement(stmt)?;
                if self.block_terminated() { break; }
            }
        }
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(merge_block).unwrap();
        }

        // Merge block
        self.builder.position_at_end(merge_block);

        Ok(())
    }

    fn generate_print(&mut self, values: &[Expression]) -> Result<(), String> {
        let printf_fn = self.printf.unwrap();
        let last = values.len().saturating_sub(1);

        for (i, expr) in values.iter().enumerate() {
            let val = self.generate_expression(expr)?;
            let newline = i == last;

            match val {
                BasicValueEnum::PointerValue(ptr) => {
                    let fmt_str = if newline { "%s\n" } else { "%s" };
                    let fmt = self.builder.build_global_string_ptr(fmt_str, "fmt_s").unwrap();
                    self.builder.build_call(printf_fn, &[fmt.as_pointer_value().into(), ptr.into()], "printf_call").unwrap();
                }
                BasicValueEnum::IntValue(int_val) => {
                    let bit_width = int_val.get_type().get_bit_width();
                    if bit_width == 64 {
                        let fmt_str = if newline { "%ld\n" } else { "%ld" };
                        let fmt = self.builder.build_global_string_ptr(fmt_str, "fmt_ld").unwrap();
                        self.builder.build_call(printf_fn, &[fmt.as_pointer_value().into(), int_val.into()], "printf_call").unwrap();
                    } else {
                        let print_val = if bit_width < 32 {
                            self.builder.build_int_z_extend(int_val, self.context.i32_type(), "zext").unwrap()
                        } else {
                            int_val
                        };
                        let fmt_str = if newline { "%d\n" } else { "%d" };
                        let fmt = self.builder.build_global_string_ptr(fmt_str, "fmt_d").unwrap();
                        self.builder.build_call(printf_fn, &[fmt.as_pointer_value().into(), print_val.into()], "printf_call").unwrap();
                    }
                }
                BasicValueEnum::FloatValue(f) => {
                    // printf expects double; promote f32 → f64
                    let f64_val = if f.get_type() == self.context.f32_type() {
                        self.builder.build_float_ext(f, self.context.f64_type(), "fpext").unwrap()
                    } else {
                        f
                    };
                    let fmt_str = if newline { "%f\n" } else { "%f" };
                    let fmt = self.builder.build_global_string_ptr(fmt_str, "fmt_f").unwrap();
                    self.builder.build_call(printf_fn, &[fmt.as_pointer_value().into(), f64_val.into()], "printf_f").unwrap();
                }
                _ => return Err("print: unsupported type".to_string()),
            }
        }

        Ok(())
    }

    fn generate_input(&mut self, name: &str, typ: &Type) -> Result<(), String> {
        let scanf_fn = self.scanf.unwrap();

        let (alloca, basic_type) = if matches!(typ, Type::Str) {
            // Allocate 256-byte buffer on the stack; build_array_alloca returns i8* directly
            let i8_type = self.context.i8_type();
            let buf_size = self.context.i32_type().const_int(256, false);
            let buf_ptr = self.builder.build_array_alloca(i8_type, buf_size, &format!("{}_buf", name)).unwrap();

            // scanf reads into the buffer
            let fmt = self.builder.build_global_string_ptr(" %255[^\n]", "fmt_scan_s").unwrap();
            self.builder.build_call(scanf_fn, &[fmt.as_pointer_value().into(), buf_ptr.into()], "scanf_s").unwrap();

            // Store the buffer pointer in a i8** slot (consistent with other str vars)
            let ptr_type = i8_type.ptr_type(AddressSpace::default());
            let var_alloca = self.builder.build_alloca(ptr_type, name).unwrap();
            self.builder.build_store(var_alloca, buf_ptr).unwrap();
            (var_alloca, BasicTypeEnum::PointerType(ptr_type))
        } else {
            let fmt_str = match typ {
                Type::I64 => "%ld",
                Type::F32 => "%f",
                Type::F64 => "%lf",
                _ => "%d",
            };
            let llvm_type = self.convert_type(typ);
            let alloca = self.builder.build_alloca(llvm_type, name).unwrap();
            let fmt = self.builder.build_global_string_ptr(fmt_str, "fmt_scan").unwrap();
            self.builder.build_call(scanf_fn, &[fmt.as_pointer_value().into(), alloca.into()], "scanf_i").unwrap();
            (alloca, llvm_type)
        };

        self.variables.insert(name.to_string(), (alloca, basic_type));
        Ok(())
    }

    fn generate_expression(&mut self, expr: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            Expression::IntLiteral(n) => {
                if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                    Ok(self.context.i32_type().const_int(*n as u64, true).into())
                } else {
                    Ok(self.context.i64_type().const_int(*n as u64, true).into())
                }
            }
            Expression::FloatLiteral(v) => {
                Ok(self.context.f64_type().const_float(*v).into())
            }
            Expression::BoolLiteral(b) => {
                Ok(self.context.bool_type().const_int(*b as u64, false).into())
            }
            Expression::ArrayLiteral(_) => {
                Err("Array literals can only appear as initializers in variable declarations".to_string())
            }
            Expression::Index { name, index } => {
                // Array parameter (pointer-based access)
                if let Some(elem_type) = self.array_params.get(name).copied() {
                    let (ptr_alloca, ptr_t) = self.variables.get(name)
                        .copied()
                        .ok_or_else(|| format!("Undefined variable: {}", name))?;
                    let ptr = self.builder.build_load(ptr_t, ptr_alloca, "arrptr")
                        .unwrap().into_pointer_value();
                    let idx = self.generate_expression(index)?.into_int_value();
                    let elem_ptr = unsafe {
                        self.builder.build_gep(elem_type, ptr, &[idx], "gep")
                    }.unwrap();
                    return Ok(self.builder.build_load(elem_type, elem_ptr, "elem").unwrap());
                }
                // Local array (ArrayType)
                let (array_ptr, array_type) = self.variables.get(name)
                    .copied()
                    .ok_or_else(|| format!("Undefined variable: {}", name))?;
                let at = match array_type {
                    BasicTypeEnum::ArrayType(at) => at,
                    _ => return Err(format!("'{}' is not an array", name)),
                };
                let element_type = at.get_element_type();
                let idx = self.generate_expression(index)?.into_int_value();
                let zero = self.context.i32_type().const_int(0, false);
                let elem_ptr = unsafe {
                    self.builder.build_gep(at, array_ptr, &[zero, idx], "gep")
                }.unwrap();
                Ok(self.builder.build_load(element_type, elem_ptr, "elem").unwrap())
            }
            Expression::Identifier(name) => {
                let (ptr, typ) = self.variables.get(name)
                    .copied()
                    .ok_or_else(|| format!("Undefined variable: {}", name))?;

                Ok(self.builder.build_load(typ, ptr, name).unwrap())
            }
            Expression::StringLiteral(s) => {
                let global = self.builder.build_global_string_ptr(s, "str_lit").unwrap();
                Ok(global.as_pointer_value().into())
            }
            Expression::Unary { op, operand } => {
                let val = self.generate_expression(operand)?;
                match op {
                    UnaryOp::Not => {
                        let iv = val.into_int_value();
                        let one = iv.get_type().const_int(1, false);
                        Ok(self.builder.build_xor(iv, one, "not").unwrap().into())
                    }
                    UnaryOp::Neg => match val {
                        BasicValueEnum::IntValue(iv) =>
                            Ok(self.builder.build_int_neg(iv, "neg").unwrap().into()),
                        BasicValueEnum::FloatValue(fv) =>
                            Ok(self.builder.build_float_neg(fv, "fneg").unwrap().into()),
                        _ => Err("Unary '-' requires numeric type".to_string()),
                    },
                }
            }
            Expression::Binary { left, op, right } => {
                self.generate_binary_expr(left, op, right)
            }
            Expression::Call { name, arguments } => {
                self.generate_call(name, arguments)
            }
            Expression::Cast { value, typ } => {
                self.generate_cast(value, typ)
            }
            Expression::StructLiteral { name, fields } => {
                let (st, field_names) = self.struct_defs.get(name)
                    .ok_or_else(|| format!("Unknown struct '{}'", name))?
                    .clone();
                let mut agg = st.const_zero();
                for (fname, fexpr) in fields {
                    let idx = field_names.iter().position(|n| n == fname)
                        .ok_or_else(|| format!("Struct '{}' has no field '{}'", name, fname))? as u32;
                    let val = self.generate_expression(fexpr)?;
                    agg = self.builder.build_insert_value(agg, val, idx, "ins")
                        .unwrap().into_struct_value();
                }
                Ok(agg.into())
            }
            Expression::FieldAccess { object, field } => {
                // Resolve the struct pointer — supports chained access (a.b.c)
                let (struct_ptr, struct_name) = self.resolve_struct_ptr(object)?;
                let (st, field_names) = self.struct_defs.get(&struct_name)
                    .ok_or_else(|| format!("Unknown struct '{}'", struct_name))?
                    .clone();
                let idx = field_names.iter().position(|n| n == field)
                    .ok_or_else(|| format!("Struct '{}' has no field '{}'", struct_name, field))? as u32;
                let field_type = st.get_field_type_at_index(idx)
                    .ok_or_else(|| format!("Field index {} out of range", idx))?;
                let field_ptr = self.builder.build_struct_gep(st, struct_ptr, idx, "fptr").unwrap();
                let val = self.builder.build_load(field_type, field_ptr, "fval").unwrap();
                Ok(val)
            }
        }
    }

    /// Resolves a struct expression to (pointer-to-struct-alloca, struct-name).
    /// Supports chained field access: `a.b.c` where b and c are struct-typed fields.
    fn resolve_struct_ptr(&mut self, expr: &Expression) -> Result<(PointerValue<'ctx>, String), String> {
        match expr {
            Expression::Identifier(name) => {
                let sname = self.var_struct_names.get(name)
                    .ok_or_else(|| format!("'{}' is not a struct variable", name))?
                    .clone();
                let (ptr, _) = *self.variables.get(name)
                    .ok_or_else(|| format!("Undefined variable: {}", name))?;
                Ok((ptr, sname))
            }
            Expression::FieldAccess { object, field } => {
                let (outer_ptr, outer_name) = self.resolve_struct_ptr(object)?;
                let (st, field_names) = self.struct_defs.get(&outer_name)
                    .ok_or_else(|| format!("Unknown struct '{}'", outer_name))?
                    .clone();
                let idx = field_names.iter().position(|n| n == field.as_str())
                    .ok_or_else(|| format!("Struct '{}' has no field '{}'", outer_name, field))? as u32;
                let field_type = st.get_field_type_at_index(idx)
                    .ok_or_else(|| format!("Field index {} out of range", idx))?;
                // Field must be a struct type to continue resolving
                match field_type {
                    BasicTypeEnum::StructType(inner_st) => {
                        let inner_name = self.struct_defs.iter()
                            .find(|(_, (t, _))| *t == inner_st)
                            .map(|(n, _)| n.clone())
                            .ok_or_else(|| format!("Cannot resolve struct name for field '{}'", field))?;
                        let field_ptr = self.builder.build_struct_gep(st, outer_ptr, idx, "nfptr").unwrap();
                        Ok((field_ptr, inner_name))
                    }
                    _ => Err(format!(
                        "Field '{}.{}' is not a struct type — cannot access sub-fields",
                        outer_name, field
                    )),
                }
            }
            _ => Err("Expected struct variable or field access expression".to_string()),
        }
    }

    fn generate_cast(&mut self, value: &Expression, typ: &Type) -> Result<BasicValueEnum<'ctx>, String> {
        let val = self.generate_expression(value)?;
        let target = self.convert_type(typ);
        match (val, target) {
            // int → int
            (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(tt)) => {
                let src_bits = iv.get_type().get_bit_width();
                let dst_bits = tt.get_bit_width();
                if dst_bits > src_bits {
                    Ok(self.builder.build_int_s_extend(iv, tt, "sext").unwrap().into())
                } else if dst_bits < src_bits {
                    Ok(self.builder.build_int_truncate(iv, tt, "trunc").unwrap().into())
                } else {
                    Ok(iv.into())
                }
            }
            // int → float
            (BasicValueEnum::IntValue(iv), BasicTypeEnum::FloatType(ft)) =>
                Ok(self.builder.build_signed_int_to_float(iv, ft, "i2f").unwrap().into()),
            // float → int
            (BasicValueEnum::FloatValue(fv), BasicTypeEnum::IntType(it)) =>
                Ok(self.builder.build_float_to_signed_int(fv, it, "f2i").unwrap().into()),
            // float → float
            (BasicValueEnum::FloatValue(fv), BasicTypeEnum::FloatType(ft)) => {
                let src = fv.get_type();
                let dst = ft;
                if src == self.context.f32_type() && dst == self.context.f64_type() {
                    Ok(self.builder.build_float_ext(fv, dst, "fpext").unwrap().into())
                } else if src == self.context.f64_type() && dst == self.context.f32_type() {
                    Ok(self.builder.build_float_trunc(fv, dst, "fptrunc").unwrap().into())
                } else {
                    Ok(fv.into())
                }
            }
            _ => Err(format!("Unsupported cast to {:?}", typ)),
        }
    }

    fn generate_call(
        &mut self,
        name: &str,
        arguments: &[Expression],
    ) -> Result<BasicValueEnum<'ctx>, String> {
        // sqrt(x) — auto-cast int to f64
        if name == "sqrt" {
            if arguments.len() != 1 { return Err("sqrt() takes 1 argument".to_string()); }
            let val = self.generate_expression(&arguments[0])?;
            let f64_val = match val {
                BasicValueEnum::FloatValue(f) => {
                    if f.get_type() == self.context.f32_type() {
                        self.builder.build_float_ext(f, self.context.f64_type(), "ext").unwrap()
                    } else { f }
                }
                BasicValueEnum::IntValue(iv) =>
                    self.builder.build_signed_int_to_float(iv, self.context.f64_type(), "i2f").unwrap(),
                _ => return Err("sqrt() requires numeric argument".to_string()),
            };
            let sqrt_fn = self.module.get_function("sqrt").unwrap();
            return Ok(self.builder.build_call(sqrt_fn, &[f64_val.into()], "sqrt").unwrap()
                .try_as_basic_value().left().unwrap());
        }

        // pow(base, exp) — auto-cast int to f64
        if name == "pow" {
            if arguments.len() != 2 { return Err("pow() takes 2 arguments".to_string()); }
            let to_f64 = |val: BasicValueEnum<'ctx>, builder: &Builder<'ctx>, ctx: &'ctx Context| -> inkwell::values::FloatValue<'ctx> {
                match val {
                    BasicValueEnum::FloatValue(f) => {
                        if f.get_type() == ctx.f32_type() {
                            builder.build_float_ext(f, ctx.f64_type(), "ext").unwrap()
                        } else { f }
                    }
                    BasicValueEnum::IntValue(iv) =>
                        builder.build_signed_int_to_float(iv, ctx.f64_type(), "i2f").unwrap(),
                    _ => panic!("pow(): non-numeric argument"),
                }
            };
            let base = to_f64(self.generate_expression(&arguments[0])?, &self.builder, self.context);
            let exp  = to_f64(self.generate_expression(&arguments[1])?, &self.builder, self.context);
            let pow_fn = self.module.get_function("pow").unwrap();
            return Ok(self.builder.build_call(pow_fn, &[base.into(), exp.into()], "pow").unwrap()
                .try_as_basic_value().left().unwrap());
        }

        // len(x) — array: compile-time size; str: strlen at runtime
        if name == "len" {
            if arguments.len() != 1 { return Err("len() takes 1 argument".to_string()); }
            if let Expression::Identifier(arr_name) = &arguments[0] {
                let (ptr, typ) = self.variables.get(arr_name).copied()
                    .ok_or_else(|| format!("Undefined variable: {}", arr_name))?;
                match typ {
                    BasicTypeEnum::ArrayType(at) => {
                        return Ok(self.context.i32_type().const_int(at.len() as u64, false).into());
                    }
                    BasicTypeEnum::PointerType(_) => {
                        let strlen = self.module.get_function("strlen").unwrap();
                        let s = self.builder.build_load(typ, ptr, arr_name).unwrap().into_pointer_value();
                        let n = self.builder.build_call(strlen, &[s.into()], "slen").unwrap()
                            .try_as_basic_value().left().unwrap().into_int_value();
                        return Ok(self.builder.build_int_truncate_or_bit_cast(n, self.context.i32_type(), "len").unwrap().into());
                    }
                    _ => {}
                }
            }

            let val = self.generate_expression(&arguments[0])?;
            match val {
                BasicValueEnum::PointerValue(s) => {
                    let strlen = self.module.get_function("strlen").unwrap();
                    let n = self.builder.build_call(strlen, &[s.into()], "slen").unwrap()
                        .try_as_basic_value().left().unwrap().into_int_value();
                    return Ok(self.builder.build_int_truncate_or_bit_cast(n, self.context.i32_type(), "len").unwrap().into());
                }
                _ => return Err("len() requires an array or str value".to_string()),
            }
        }

        // str_to_int(s), str_to_float(s)
        if name == "str_to_int" {
            if arguments.len() != 1 { return Err("str_to_int() takes 1 argument".to_string()); }
            let s = self.generate_expression(&arguments[0])?.into_pointer_value();
            let atoi = self.module.get_function("atoi").unwrap();
            return Ok(self.builder.build_call(atoi, &[s.into()], "atoi").unwrap()
                .try_as_basic_value().left().unwrap());
        }
        if name == "str_to_float" {
            if arguments.len() != 1 { return Err("str_to_float() takes 1 argument".to_string()); }
            let s = self.generate_expression(&arguments[0])?.into_pointer_value();
            let atof = self.module.get_function("atof").unwrap();
            return Ok(self.builder.build_call(atof, &[s.into()], "atof").unwrap()
                .try_as_basic_value().left().unwrap());
        }

        // int_to_str(n) — sprintf into malloc'd buffer
        if name == "int_to_str" {
            if arguments.len() != 1 { return Err("int_to_str() takes 1 argument".to_string()); }
            let val = self.generate_expression(&arguments[0])?;
            let malloc   = self.module.get_function("__vit_req_alloc").unwrap();
            let sprintf  = self.module.get_function("sprintf").unwrap();
            let buf = self.builder.build_call(malloc, &[self.context.i64_type().const_int(32, false).into()], "buf").unwrap()
                .try_as_basic_value().left().unwrap().into_pointer_value();
            let fmt = match val {
                BasicValueEnum::IntValue(iv) if iv.get_type().get_bit_width() == 64 => {
                    let f = self.builder.build_global_string_ptr("%ld", "fmt_ld").unwrap();
                    self.builder.build_call(sprintf, &[buf.into(), f.as_pointer_value().into(), iv.into()], "sp").unwrap();
                }
                BasicValueEnum::IntValue(iv) => {
                    let f = self.builder.build_global_string_ptr("%d", "fmt_d").unwrap();
                    self.builder.build_call(sprintf, &[buf.into(), f.as_pointer_value().into(), iv.into()], "sp").unwrap();
                }
                BasicValueEnum::FloatValue(fv) => {
                    let f64v = if fv.get_type() == self.context.f32_type() {
                        self.builder.build_float_ext(fv, self.context.f64_type(), "ext").unwrap()
                    } else { fv };
                    let f = self.builder.build_global_string_ptr("%f", "fmt_f").unwrap();
                    self.builder.build_call(sprintf, &[buf.into(), f.as_pointer_value().into(), f64v.into()], "sp").unwrap();
                }
                _ => return Err("int_to_str() requires a numeric argument".to_string()),
            };
            return Ok(buf.into());
        }

        // sort(arr, n) — qsort with type-appropriate comparator
        if name == "sort" {
            if arguments.len() != 2 { return Err("sort() takes 2 arguments: sort(array, n)".to_string()); }
            let arr_name = match &arguments[0] {
                Expression::Identifier(n) => n.clone(),
                _ => return Err("First argument of sort() must be an array variable".to_string()),
            };
            let n_val = self.generate_expression(&arguments[1])?.into_int_value();
            let n_i64 = self.builder.build_int_s_extend_or_bit_cast(n_val, self.context.i64_type(), "n64").unwrap();
            let qsort = self.module.get_function("qsort").unwrap();

            if let Some(elem_type) = self.array_params.get(&arr_name).copied() {
                // Array parameter: already a pointer to first element
                let (ptr_alloca, ptr_t) = self.variables.get(&arr_name).copied()
                    .ok_or_else(|| format!("Undefined variable: {}", arr_name))?;
                let ptr = self.builder.build_load(ptr_t, ptr_alloca, "arrptr")
                    .unwrap().into_pointer_value();
                let (elem_size, cmp_name) = match elem_type {
                    BasicTypeEnum::IntType(t) if t.get_bit_width() == 64 => (8u64, "__vit_cmp_i64"),
                    BasicTypeEnum::FloatType(_)                          => (8u64, "__vit_cmp_f64"),
                    _                                                    => (4u64, "__vit_cmp_i32"),
                };
                let cmp_fn = self.module.get_function(cmp_name).unwrap();
                let cmp_ptr = cmp_fn.as_global_value().as_pointer_value();
                let size_val = self.context.i64_type().const_int(elem_size, false);
                self.builder.build_call(qsort, &[ptr.into(), n_i64.into(), size_val.into(), cmp_ptr.into()], "").unwrap();
            } else {
                // Local array
                let (arr_ptr, arr_type) = self.variables.get(&arr_name).copied()
                    .ok_or_else(|| format!("Undefined variable: {}", arr_name))?;
                let at = match arr_type {
                    BasicTypeEnum::ArrayType(at) => at,
                    _ => return Err(format!("'{}' is not an array", arr_name)),
                };
                let (elem_size, cmp_name) = match at.get_element_type() {
                    BasicTypeEnum::IntType(t) if t.get_bit_width() == 64 => (8u64, "__vit_cmp_i64"),
                    BasicTypeEnum::FloatType(_)                          => (8u64, "__vit_cmp_f64"),
                    _                                                    => (4u64, "__vit_cmp_i32"),
                };
                let cmp_fn = self.module.get_function(cmp_name).unwrap();
                let cmp_ptr = cmp_fn.as_global_value().as_pointer_value();
                let size_val = self.context.i64_type().const_int(elem_size, false);
                let i32_type = self.context.i32_type();
                let zero = i32_type.const_int(0, false);
                let first = unsafe { self.builder.build_gep(at, arr_ptr, &[zero, zero], "first") }.unwrap();
                self.builder.build_call(qsort, &[first.into(), n_i64.into(), size_val.into(), cmp_ptr.into()], "").unwrap();
            }

            // sort() returns void — return i32 0 as dummy so it can be used as statement
            return Ok(self.context.i32_type().const_int(0, false).into());
        }

        // abs(x), min(x,y), max(x,y) — branchless via select
        if name == "abs" {
            if arguments.len() != 1 { return Err("abs() takes 1 argument".to_string()); }
            let val = self.generate_expression(&arguments[0])?;
            return match val {
                BasicValueEnum::IntValue(iv) => {
                    let zero = iv.get_type().const_int(0, false);
                    let neg  = self.builder.build_int_neg(iv, "neg").unwrap();
                    let cmp  = self.builder.build_int_compare(IntPredicate::SGE, iv, zero, "cmp").unwrap();
                    Ok(self.builder.build_select(cmp, iv, neg, "abs").unwrap().into())
                }
                BasicValueEnum::FloatValue(fv) => {
                    let zero = fv.get_type().const_float(0.0);
                    let neg  = self.builder.build_float_neg(fv, "fneg").unwrap();
                    let cmp  = self.builder.build_float_compare(FloatPredicate::OGE, fv, zero, "fcmp").unwrap();
                    Ok(self.builder.build_select(cmp, fv, neg, "fabs").unwrap().into())
                }
                _ => Err("abs() requires numeric argument".to_string()),
            };
        }
        if name == "min" || name == "max" {
            if arguments.len() != 2 { return Err(format!("{}() takes 2 arguments", name)); }
            let lhs = self.generate_expression(&arguments[0])?;
            let rhs = self.generate_expression(&arguments[1])?;
            return match (lhs, rhs) {
                (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => {
                    let cmp = if name == "min" {
                        self.builder.build_int_compare(IntPredicate::SLT, l, r, "cmp").unwrap()
                    } else {
                        self.builder.build_int_compare(IntPredicate::SGT, l, r, "cmp").unwrap()
                    };
                    Ok(self.builder.build_select(cmp, l, r, name).unwrap().into())
                }
                (BasicValueEnum::FloatValue(l), BasicValueEnum::FloatValue(r)) => {
                    let cmp = if name == "min" {
                        self.builder.build_float_compare(FloatPredicate::OLT, l, r, "fcmp").unwrap()
                    } else {
                        self.builder.build_float_compare(FloatPredicate::OGT, l, r, "fcmp").unwrap()
                    };
                    Ok(self.builder.build_select(cmp, l, r, name).unwrap().into())
                }
                _ => Err(format!("{}() requires two arguments of the same numeric type", name)),
            };
        }

        // map_set(m, k, v) / map_get(m, k) / map_has(m, k)
        if name == "map_set" || name == "map_get" || name == "map_has" {
            let expected = if name == "map_set" { 3 } else { 2 };
            if arguments.len() != expected {
                return Err(format!("{}() takes {} arguments", name, expected));
            }
            let (key_type, val_type, map_ptr) = match &arguments[0] {
                Expression::Identifier(map_name) => {
                    let (key_type, val_type) = self.map_variables.get(map_name).cloned()
                        .ok_or_else(|| format!("'{}' is not a map variable", map_name))?;
                    let (alloca, atyp) = self.variables.get(map_name).copied()
                        .ok_or_else(|| format!("Undefined variable: {}", map_name))?;
                    let map_ptr = self.builder.build_load(atyp, alloca, map_name).unwrap().into_pointer_value();
                    (key_type, val_type, map_ptr)
                }
                Expression::FieldAccess { object, field } => {
                    let (struct_ptr, struct_name) = self.resolve_struct_ptr(object)?;
                    let (_, field_names) = self.struct_defs.get(&struct_name)
                        .ok_or_else(|| format!("Unknown struct '{}'", struct_name))?
                        .clone();
                    let field_types = self.struct_field_types.get(&struct_name)
                        .ok_or_else(|| format!("Missing field type info for struct '{}'", struct_name))?;
                    let idx = field_names.iter().position(|n| n == field)
                        .ok_or_else(|| format!("Struct '{}' has no field '{}'", struct_name, field))?;
                    let (key_type, val_type) = match &field_types[idx] {
                        Type::Map { key, value } => ((*key.clone()), (*value.clone())),
                        _ => return Err(format!("First arg of {}() must be a map variable", name)),
                    };
                    let map_ptr = self.generate_expression(&arguments[0])?.into_pointer_value();
                    (key_type, val_type, map_ptr)
                }
                _ => return Err(format!("First arg of {}() must be a map variable", name)),
            };

            let helper_name = match (name, &key_type, &val_type) {
                ("map_set", Type::I32, Type::I32) => "__vit_map_i32i32_set",
                ("map_get", Type::I32, Type::I32) => "__vit_map_i32i32_get",
                ("map_has", Type::I32, Type::I32) => "__vit_map_i32i32_has",
                ("map_set", Type::Str, Type::I32) => "__vit_map_stri32_set",
                ("map_get", Type::Str, Type::I32) => "__vit_map_stri32_get",
                ("map_has", Type::Str, Type::I32) => "__vit_map_stri32_has",
                ("map_set", Type::Str, Type::Str) => "__vit_map_strstr_set",
                ("map_get", Type::Str, Type::Str) => "__vit_map_strstr_get",
                ("map_has", Type::Str, Type::Str) => "__vit_map_strstr_has",
                ("map_set", Type::I32, Type::I64) => "__vit_map_i32i64_set",
                ("map_get", Type::I32, Type::I64) => "__vit_map_i32i64_get",
                ("map_has", Type::I32, Type::I64) => "__vit_map_i32i64_has",
                ("map_set", Type::I64, Type::I32) => "__vit_map_i64i32_set",
                ("map_get", Type::I64, Type::I32) => "__vit_map_i64i32_get",
                ("map_has", Type::I64, Type::I32) => "__vit_map_i64i32_has",
                ("map_set", Type::I64, Type::I64) => "__vit_map_i64i64_set",
                ("map_get", Type::I64, Type::I64) => "__vit_map_i64i64_get",
                ("map_has", Type::I64, Type::I64) => "__vit_map_i64i64_has",
                _ => return Err(format!("Unsupported map type for {}: map[{}, {}]", name, key_type, val_type)),
            };
            let helper = self.module.get_function(helper_name).unwrap();
            let mut args: Vec<BasicMetadataValueEnum> = vec![map_ptr.into()];
            for arg in &arguments[1..] {
                args.push(self.generate_expression(arg)?.into());
            }
            let result = self.builder.build_call(helper, &args, "map_call").unwrap();
            let raw = result.try_as_basic_value().left()
                .unwrap_or_else(|| self.context.i32_type().const_int(0, false).into());
            // map_has must return i1 (bool) for use as branch condition
            if name == "map_has" {
                let iv  = raw.into_int_value();
                let b   = self.builder.build_int_compare(
                    IntPredicate::NE, iv, self.context.i32_type().const_int(0, false), "has"
                ).unwrap();
                return Ok(b.into());
            }
            return Ok(raw);
        }

        // map_free(m)
        // Frees the backing allocation of a local/global map variable.
        if name == "map_free" {
            if arguments.len() != 1 {
                return Err("map_free() takes 1 argument".to_string());
            }

            let map_ptr = self.generate_expression(&arguments[0])?.into_pointer_value();
            let free_fn = self.module.get_function("free").unwrap();
            self.builder.build_call(free_fn, &[map_ptr.into()], "map_free").unwrap();
            return Ok(self.context.i32_type().const_int(0, false).into());
        }

        // free(ptr)
        // Frees a heap-allocated string/buffer previously returned by Vit helpers or C shims.
        if name == "free" {
            if arguments.len() != 1 {
                return Err("free() takes 1 argument".to_string());
            }

            let ptr = self.generate_expression(&arguments[0])?.into_pointer_value();
            let free_fn = self.module.get_function("__vit_req_free").unwrap();
            self.builder.build_call(free_fn, &[ptr.into()], "free_call").unwrap();
            return Ok(self.context.i32_type().const_int(0, false).into());
        }

        // malloc(size: i32) -> str
        // Calls C malloc, auto-extends i32 to i64.
        if name == "malloc" {
            if arguments.len() != 1 {
                return Err("malloc() takes 1 argument".to_string());
            }
            let i64_type  = self.context.i64_type();
            let malloc_fn = self.module.get_function("malloc").unwrap();
            let size_val  = self.generate_expression(&arguments[0])?.into_int_value();
            let size64    = self.builder.build_int_s_extend_or_bit_cast(size_val, i64_type, "malloc_sz").unwrap();
            return Ok(self.builder.build_call(malloc_fn, &[size64.into()], "malloc_buf")
                .unwrap().try_as_basic_value().left().unwrap());
        }

        // format(fmt, arg1, arg2, ...) -> str
        // Calls sprintf into a malloc'd 4096-byte buffer and returns it.
        if name == "format" {
            if arguments.is_empty() {
                return Err("format() requires at least a format string".to_string());
            }
            let i64_type  = self.context.i64_type();
            let malloc_fn = self.module.get_function("__vit_req_alloc").unwrap();
            let sprintf   = self.module.get_function("sprintf").unwrap();

            let buf = self.builder.build_call(
                malloc_fn,
                &[i64_type.const_int(4096, false).into()],
                "fmt_buf",
            ).unwrap().try_as_basic_value().left().unwrap().into_pointer_value();

            let fmt_val = self.generate_expression(&arguments[0])?.into_pointer_value();
            let mut sprintf_args: Vec<BasicMetadataValueEnum> =
                vec![buf.into(), fmt_val.into()];
            for arg in &arguments[1..] {
                sprintf_args.push(self.generate_expression(arg)?.into());
            }
            self.builder.build_call(sprintf, &sprintf_args, "fmt_res").unwrap();
            return Ok(buf.into());
        }

        // substr(s, start, len) -> str
        // Returns a malloc'd copy of s[start..start+len], null-terminated.
        if name == "substr" {
            if arguments.len() != 3 {
                return Err("substr() takes 3 arguments: substr(s, start, len)".to_string());
            }
            let i8_type   = self.context.i8_type();
            let i32_type  = self.context.i32_type();
            let i64_type  = self.context.i64_type();
            let i8_ptr    = i8_type.ptr_type(AddressSpace::default());
            let malloc_fn = self.module.get_function("__vit_req_alloc").unwrap();
            let strncpy   = self.module.get_function("strncpy").unwrap();

            let s_val     = self.generate_expression(&arguments[0])?.into_pointer_value();
            let start_val = self.generate_expression(&arguments[1])?.into_int_value();
            let len_val   = self.generate_expression(&arguments[2])?.into_int_value();

            // start and len may be i32 — extend to i64 for GEP/malloc
            let start64 = self.builder.build_int_s_extend_or_bit_cast(start_val, i64_type, "start64").unwrap();
            let len64   = self.builder.build_int_s_extend_or_bit_cast(len_val, i64_type, "len64").unwrap();

            // malloc(len + 1)
            let one64   = i64_type.const_int(1, false);
            let buf_size = self.builder.build_int_add(len64, one64, "buf_size").unwrap();
            let buf = self.builder.build_call(malloc_fn, &[buf_size.into()], "sub_buf")
                .unwrap().try_as_basic_value().left().unwrap().into_pointer_value();

            // src = s + start  (GEP on i8*)
            let src = unsafe {
                self.builder.build_gep(i8_type, s_val, &[start64.into()], "src_ptr").unwrap()
            };

            // strncpy(buf, src, len)
            self.builder.build_call(strncpy, &[buf.into(), src.into(), len64.into()], "sncpy").unwrap();

            // null-terminate: buf[len] = 0
            let null_pos = unsafe {
                self.builder.build_gep(i8_type, buf, &[len64.into()], "null_pos").unwrap()
            };
            self.builder.build_store(null_pos, i8_type.const_int(0, false)).unwrap();

            return Ok(buf.into());
        }

        // str_pos(s, needle) -> i32
        // Returns the byte offset of the first occurrence of needle in s, or -1 if not found.
        if name == "str_pos" {
            if arguments.len() != 2 {
                return Err("str_pos() takes 2 arguments: str_pos(s, needle)".to_string());
            }
            let i8_type  = self.context.i8_type();
            let i32_type = self.context.i32_type();
            let i64_type = self.context.i64_type();
            let strstr   = self.module.get_function("strstr").unwrap();

            let s_val      = self.generate_expression(&arguments[0])?.into_pointer_value();
            let needle_val = self.generate_expression(&arguments[1])?.into_pointer_value();

            // found = strstr(s, needle)
            let found = self.builder.build_call(strstr, &[s_val.into(), needle_val.into()], "found")
                .unwrap().try_as_basic_value().left().unwrap().into_pointer_value();

            // Check found == null
            let null_ptr = i8_type.ptr_type(AddressSpace::default()).const_null();
            let is_null  = self.builder.build_is_null(found, "is_null").unwrap();

            // offset = (intptr_t)found - (intptr_t)s  (ptrtoint both, subtract)
            let found_int = self.builder.build_ptr_to_int(found, i64_type, "found_int").unwrap();
            let s_int     = self.builder.build_ptr_to_int(s_val,  i64_type, "s_int").unwrap();
            let offset64  = self.builder.build_int_sub(found_int, s_int, "offset64").unwrap();
            let offset32  = self.builder.build_int_truncate_or_bit_cast(offset64, i32_type, "offset32").unwrap();

            let neg_one  = i32_type.const_int(u64::MAX, true); // -1 as i32
            let result   = self.builder.build_select(is_null, neg_one, offset32, "str_pos_res").unwrap();

            return Ok(result.into());
        }

        // split(s, sep, arr) is handled specially — needs to extract array ptr and size
        if name == "split" {
            if arguments.len() != 3 {
                return Err("split() takes 3 arguments: split(string, sep, array)".to_string());
            }
            let s_val   = self.generate_expression(&arguments[0])?.into_pointer_value();
            let sep_val = self.generate_expression(&arguments[1])?.into_pointer_value();

            let arr_name = match &arguments[2] {
                Expression::Identifier(n) => n.clone(),
                _ => return Err("Third argument of split() must be an array variable".to_string()),
            };
            let (arr_ptr, arr_type) = self.variables.get(&arr_name).copied()
                .ok_or_else(|| format!("Undefined variable: {}", arr_name))?;
            let at = match arr_type {
                BasicTypeEnum::ArrayType(at) => at,
                _ => return Err(format!("'{}' is not an array", arr_name)),
            };
            let max_size = at.len();

            // GEP [0, 0] to get pointer to first element (i8**)
            let i32_type = self.context.i32_type();
            let zero = i32_type.const_int(0, false);
            let first_elem = unsafe {
                self.builder.build_gep(at, arr_ptr, &[zero, zero], "sarr")
            }.unwrap();

            let max_val = i32_type.const_int(max_size as u64, false);
            let split_fn = self.module.get_function("__vit_split").unwrap();
            let result = self.builder.build_call(
                split_fn,
                &[s_val.into(), sep_val.into(), first_elem.into(), max_val.into()],
                "split_res",
            ).unwrap();
            return Ok(result.try_as_basic_value().left().unwrap());
        }

        // http_handle("METHOD", "/path", handler_fn)
        // Registers a route in the global route table.
        if name == "http_handle" {
            if arguments.len() != 3 {
                return Err("http_handle() takes 3 arguments: method, path, handler".to_string());
            }

            let i32_type = self.context.i32_type();
            let i8_ptr   = self.context.i8_type().ptr_type(AddressSpace::default());
            let arr64    = i8_ptr.array_type(64);

            let method_val = self.generate_expression(&arguments[0])?.into_pointer_value();
            let path_val   = self.generate_expression(&arguments[1])?.into_pointer_value();

            let handler_name = match &arguments[2] {
                Expression::Identifier(n) => n.clone(),
                _ => return Err("http_handle(): third argument must be a function name".to_string()),
            };
            let handler_ptr = self.ensure_http_handler_wrapper(&handler_name)?;

            let count_g   = self.module.get_global("__vit_route_count").unwrap().as_pointer_value();
            let methods_g = self.module.get_global("__vit_route_methods").unwrap().as_pointer_value();
            let paths_g   = self.module.get_global("__vit_route_paths").unwrap().as_pointer_value();
            let handlers_g= self.module.get_global("__vit_route_handlers").unwrap().as_pointer_value();
            let zero       = i32_type.const_int(0, false);

            let count = self.builder.build_load(i32_type, count_g, "cnt").unwrap().into_int_value();

            let mp = unsafe { self.builder.build_gep(arr64, methods_g,  &[zero, count], "mp") }.unwrap();
            self.builder.build_store(mp, method_val).unwrap();

            let pp = unsafe { self.builder.build_gep(arr64, paths_g,    &[zero, count], "pp") }.unwrap();
            self.builder.build_store(pp, path_val).unwrap();

            let hp = unsafe { self.builder.build_gep(arr64, handlers_g, &[zero, count], "hp") }.unwrap();
            self.builder.build_store(hp, handler_ptr).unwrap();

            let new_count = self.builder.build_int_add(count, i32_type.const_int(1, false), "cnt1").unwrap();
            self.builder.build_store(count_g, new_count).unwrap();

            return Ok(i32_type.const_int(0, false).into());
        }
        // http_listen(port)
        // Starts the accept loop and dispatches requests using the registered route table.
        if name == "http_listen" || name == "http_listen_fork" {
            if arguments.len() != 1 {
                return Err(format!("{}() takes 1 argument: port", name));
            }
            let use_fork = name == "http_listen_fork";

            let port_val = self.generate_expression(&arguments[0])?.into_int_value();

            let i32_type = self.context.i32_type();
            let i64_type = self.context.i64_type();
            let i8_type  = self.context.i8_type();
            let i8_ptr   = i8_type.ptr_type(AddressSpace::default());
            let arr64    = i8_ptr.array_type(64);

            let tcp_listen_fn = self.module.get_function("__vit_tcp_listen").unwrap();
            let tcp_accept_fn = self.module.get_function("__vit_tcp_accept").unwrap();
            let tcp_close_fn  = self.module.get_function("__vit_tcp_close").unwrap();
            let strcmp_fn     = self.module.get_function("strcmp").unwrap();
            let free_fn       = self.module.get_function("free").unwrap();
            let req_begin_fn  = self.module.get_function("__vit_req_begin").unwrap();
            let req_end_fn    = self.module.get_function("__vit_req_end").unwrap();
            let fork_fn       = self.module.get_function("fork").unwrap();
            let signal_fn     = self.module.get_function("signal").unwrap();
            let exit_fn       = self.module.get_function("_exit").unwrap();
            let http_read_fn = self.module.get_function("http_read")
                .ok_or_else(|| "http_listen(): http_read not found - did you import lib/http.vit?".to_string())?;
            let http_send_fn = self.module.get_function("http_send")
                .ok_or_else(|| "http_listen(): http_send not found - did you import lib/http.vit?".to_string())?;
            let http_not_found_fn = self.module.get_function("http_not_found")
                .ok_or_else(|| "http_listen(): http_not_found not found - did you import lib/http.vit?".to_string())?;
            let http_request_free_fn = self.module.get_function("http_request_free")
                .ok_or_else(|| "http_listen(): http_request_free not found - did you import lib/http.vit?".to_string())?;
            let http_route_matches_fn = self.module.get_function("http_route_matches")
                .ok_or_else(|| "http_listen(): http_route_matches not found - did you import lib/http.vit?".to_string())?;
            let http_route_apply_fn = self.module.get_function("http_route_apply")
                .ok_or_else(|| "http_listen(): http_route_apply not found - did you import lib/http.vit?".to_string())?;
            let http_parse_fn = self.module.get_function("http_parse")
                .ok_or_else(|| "http_listen(): http_parse not found - did you import lib/http.vit?".to_string())?;

            let (req_st_type, req_fields) = self.struct_defs.get("Request").cloned()
                .ok_or_else(|| "http_listen(): Request struct not found - did you import lib/http.vit?".to_string())?;

            let count_g    = self.module.get_global("__vit_route_count").unwrap().as_pointer_value();
            let methods_g  = self.module.get_global("__vit_route_methods").unwrap().as_pointer_value();
            let paths_g    = self.module.get_global("__vit_route_paths").unwrap().as_pointer_value();
            let handlers_g = self.module.get_global("__vit_route_handlers").unwrap().as_pointer_value();
            let zero32     = i32_type.const_int(0, false);

            let server_fd = self.builder
                .build_call(tcp_listen_fn, &[port_val.into()], "srv_fd")
                .unwrap().try_as_basic_value().left().unwrap().into_int_value();
            let srv_slot = self.builder.build_alloca(i32_type, "srv_slot").unwrap();
            let cli_slot = self.builder.build_alloca(i32_type, "cli_slot").unwrap();
            let req_slot = self.builder.build_alloca(req_st_type, "req_slot").unwrap();
            let i_slot = self.builder.build_alloca(i32_type, "i_slot").unwrap();
            let resp_slot = self.builder.build_alloca(i8_ptr, "resp_slot").unwrap();
            self.builder.build_store(srv_slot, server_fd).unwrap();
            if use_fork {
                let sig_ign = self.builder
                    .build_int_to_ptr(i64_type.const_int(1, false), i8_ptr, "sig_ign")
                    .unwrap();
                self.builder.build_call(
                    signal_fn,
                    &[i32_type.const_int(17, false).into(), sig_ign.into()],
                    "sigchld_ign"
                ).unwrap();
            }

            let current_fn = self.current_function.unwrap();
            let accept_bb  = self.context.append_basic_block(current_fn, "http_accept");
            let read_bb            = self.context.append_basic_block(current_fn, "http_read");
            let dispatch_check_bb  = self.context.append_basic_block(current_fn, "dispatch_check");
            let dispatch_body_bb   = self.context.append_basic_block(current_fn, "dispatch_body");
            let check_path_bb      = self.context.append_basic_block(current_fn, "check_path");
            let dispatch_match_bb  = self.context.append_basic_block(current_fn, "dispatch_match");
            let dispatch_next_bb   = self.context.append_basic_block(current_fn, "dispatch_next");
            let dispatch_done_bb   = self.context.append_basic_block(current_fn, "dispatch_done");
            let send_bb            = self.context.append_basic_block(current_fn, "http_send");
            let after_bb           = self.context.append_basic_block(current_fn, "http_after");
            let fork_parent_check_bb = if use_fork {
                Some(self.context.append_basic_block(current_fn, "fork_parent_check"))
            } else { None };
            let fork_parent_bb = if use_fork {
                Some(self.context.append_basic_block(current_fn, "fork_parent"))
            } else { None };
            let fork_fail_bb = if use_fork {
                Some(self.context.append_basic_block(current_fn, "fork_fail"))
            } else { None };
            let fork_child_bb = if use_fork {
                Some(self.context.append_basic_block(current_fn, "fork_child"))
            } else { None };
            let pid_slot = if use_fork {
                Some(self.builder.build_alloca(i32_type, "pid_slot").unwrap())
            } else { None };

            self.builder.build_unconditional_branch(accept_bb).unwrap();

            self.builder.position_at_end(accept_bb);
            let srv = self.builder.build_load(i32_type, srv_slot, "srv").unwrap().into_int_value();
            let client_fd = self.builder
                .build_call(tcp_accept_fn, &[srv.into()], "cli")
                .unwrap().try_as_basic_value().left().unwrap().into_int_value();
            self.builder.build_store(cli_slot, client_fd).unwrap();
            if use_fork {
                let pid = self.builder
                    .build_call(fork_fn, &[], "pid")
                    .unwrap().try_as_basic_value().left().unwrap().into_int_value();
                self.builder.build_store(pid_slot.unwrap(), pid).unwrap();
                let is_child = self.builder.build_int_compare(
                    IntPredicate::EQ,
                    pid,
                    i32_type.const_zero(),
                    "is_child"
                ).unwrap();
                self.builder.build_conditional_branch(
                    is_child,
                    fork_child_bb.unwrap(),
                    fork_parent_check_bb.unwrap()
                ).unwrap();
            } else {
                self.builder.build_call(req_begin_fn, &[], "").unwrap();
                self.builder.build_unconditional_branch(read_bb).unwrap();
            }

            if use_fork {
                self.builder.position_at_end(fork_parent_check_bb.unwrap());
                let pid = self.builder.build_load(i32_type, pid_slot.unwrap(), "pid_parent").unwrap().into_int_value();
                let fork_failed = self.builder.build_int_compare(
                    IntPredicate::SLT,
                    pid,
                    i32_type.const_zero(),
                    "fork_failed"
                ).unwrap();
                self.builder.build_conditional_branch(
                    fork_failed,
                    fork_fail_bb.unwrap(),
                    fork_parent_bb.unwrap()
                ).unwrap();

                self.builder.position_at_end(fork_parent_bb.unwrap());
                let cli = self.builder.build_load(i32_type, cli_slot, "cli_parent").unwrap().into_int_value();
                self.builder.build_call(tcp_close_fn, &[cli.into()], "close_parent").unwrap();
                self.builder.build_unconditional_branch(accept_bb).unwrap();

                self.builder.position_at_end(fork_fail_bb.unwrap());
                let cli = self.builder.build_load(i32_type, cli_slot, "cli_fail").unwrap().into_int_value();
                self.builder.build_call(tcp_close_fn, &[cli.into()], "close_fail").unwrap();
                self.builder.build_unconditional_branch(accept_bb).unwrap();

                self.builder.position_at_end(fork_child_bb.unwrap());
                let child_srv = self.builder.build_load(i32_type, srv_slot, "child_srv").unwrap().into_int_value();
                self.builder.build_call(tcp_close_fn, &[child_srv.into()], "close_child_srv").unwrap();
                self.builder.build_call(req_begin_fn, &[], "").unwrap();
                self.builder.build_unconditional_branch(read_bb).unwrap();
            }

            self.builder.position_at_end(read_bb);
            let buf = self.builder
                .build_call(http_read_fn, &[client_fd.into()], "rbuf")
                .unwrap().try_as_basic_value().left().unwrap().into_pointer_value();

            let req_val = self.builder
                .build_call(http_parse_fn, &[buf.into()], "req_val")
                .unwrap().try_as_basic_value().left().unwrap();
            self.builder.build_call(free_fn, &[buf.into()], "free_http_buf").unwrap();
            self.builder.build_store(req_slot, req_val.into_struct_value()).unwrap();

            self.builder.build_store(i_slot, zero32).unwrap();
            self.builder.build_store(resp_slot, i8_ptr.const_null()).unwrap();
            self.builder.build_unconditional_branch(dispatch_check_bb).unwrap();

            self.builder.position_at_end(dispatch_check_bb);
            let i   = self.builder.build_load(i32_type, i_slot, "i").unwrap().into_int_value();
            let cnt = self.builder.build_load(i32_type, count_g, "cnt").unwrap().into_int_value();
            let exhausted = self.builder.build_int_compare(IntPredicate::SGE, i, cnt, "exh").unwrap();
            self.builder.build_conditional_branch(exhausted, dispatch_done_bb, dispatch_body_bb).unwrap();

            self.builder.position_at_end(dispatch_body_bb);
            let i2 = self.builder.build_load(i32_type, i_slot, "i2").unwrap().into_int_value();
            let method_idx = req_fields.iter().position(|f| f == "method").unwrap_or(0) as u32;
            let method_gep = self.builder
                .build_struct_gep(req_st_type, req_slot, method_idx, "req_method_ptr").unwrap();
            let req_method = self.builder.build_load(i8_ptr, method_gep, "req_method").unwrap().into_pointer_value();

            let mptr = unsafe { self.builder.build_gep(arr64, methods_g, &[zero32, i2], "mptr") }.unwrap();
            let route_method = self.builder.build_load(i8_ptr, mptr, "rm").unwrap().into_pointer_value();

            let mcmp = self.builder
                .build_call(strcmp_fn, &[req_method.into(), route_method.into()], "mcmp")
                .unwrap().try_as_basic_value().left().unwrap().into_int_value();
            let meq = self.builder.build_int_compare(IntPredicate::EQ, mcmp, zero32, "meq").unwrap();
            self.builder.build_conditional_branch(meq, check_path_bb, dispatch_next_bb).unwrap();

            self.builder.position_at_end(check_path_bb);
            let i3 = self.builder.build_load(i32_type, i_slot, "i3").unwrap().into_int_value();
            let pptr = unsafe { self.builder.build_gep(arr64, paths_g, &[zero32, i3], "pptr") }.unwrap();
            let route_path = self.builder.build_load(i8_ptr, pptr, "rp").unwrap().into_pointer_value();

            let route_match = self.builder
                .build_call(http_route_matches_fn, &[req_slot.into(), route_path.into()], "route_match")
                .unwrap().try_as_basic_value().left().unwrap().into_int_value();
            let route_applied = self.builder
                .build_call(http_route_apply_fn, &[req_slot.into(), route_path.into()], "route_applied")
                .unwrap().try_as_basic_value().left().unwrap();
            self.builder.build_store(req_slot, route_applied.into_struct_value()).unwrap();
            let matched = self.builder.build_int_compare(IntPredicate::NE, route_match, zero32, "matched").unwrap();
            self.builder.build_conditional_branch(matched, dispatch_match_bb, dispatch_next_bb).unwrap();

            self.builder.position_at_end(dispatch_match_bb);
            let i4 = self.builder.build_load(i32_type, i_slot, "i4").unwrap().into_int_value();
            let hptr = unsafe { self.builder.build_gep(arr64, handlers_g, &[zero32, i4], "hptr") }.unwrap();
            let handler_ptr_val = self.builder.build_load(i8_ptr, hptr, "hfn").unwrap().into_pointer_value();

            let handler_fn_type = i8_ptr.fn_type(&[i8_ptr.into()], false);
            let resp = self.builder
                .build_indirect_call(handler_fn_type, handler_ptr_val, &[req_slot.into()], "resp")
                .unwrap().try_as_basic_value().left().unwrap().into_pointer_value();
            self.builder.build_store(resp_slot, resp).unwrap();
            self.builder.build_unconditional_branch(dispatch_done_bb).unwrap();

            self.builder.position_at_end(dispatch_next_bb);
            let i5  = self.builder.build_load(i32_type, i_slot, "i5").unwrap().into_int_value();
            let i5p = self.builder.build_int_add(i5, i32_type.const_int(1, false), "i5p").unwrap();
            self.builder.build_store(i_slot, i5p).unwrap();
            self.builder.build_unconditional_branch(dispatch_check_bb).unwrap();

            self.builder.position_at_end(dispatch_done_bb);
            let resp_val = self.builder.build_load(i8_ptr, resp_slot, "respv").unwrap().into_pointer_value();
            let is_null  = self.builder.build_is_null(resp_val, "isnull").unwrap();
            let not_found_str = self.builder
                .build_call(http_not_found_fn, &[], "nf_str")
                .unwrap().try_as_basic_value().left().unwrap().into_pointer_value();
            let final_resp = self.builder
                .build_select(is_null, not_found_str, resp_val, "final_resp").unwrap()
                .into_pointer_value();
            self.builder.build_store(resp_slot, final_resp).unwrap();
            self.builder.build_unconditional_branch(send_bb).unwrap();

            self.builder.position_at_end(send_bb);
            let final_resp2 = self.builder.build_load(i8_ptr, resp_slot, "final_resp2").unwrap().into_pointer_value();
            let cli  = self.builder.build_load(i32_type, cli_slot, "cli2").unwrap().into_int_value();
            self.builder.build_call(http_send_fn, &[cli.into(), final_resp2.into()], "nsent").unwrap();
            self.builder.build_call(http_request_free_fn, &[req_slot.into()], "free_req").unwrap();
            self.builder.build_call(req_end_fn, &[], "").unwrap();
            self.builder.build_call(tcp_close_fn, &[cli.into()], "closed").unwrap();
            if use_fork {
                self.builder.build_call(exit_fn, &[i32_type.const_zero().into()], "").unwrap();
                self.builder.build_unreachable().unwrap();
            } else {
                self.builder.build_unconditional_branch(accept_bb).unwrap();
            }

            self.builder.position_at_end(after_bb);
            return Ok(i32_type.const_int(0, false).into());
        }

        // Built-in string functions are compiled as __vit_* helpers
        let internal_name = match name {
            "add" | "remove" | "replace" => format!("__vit_{}", name),
            _ => name.to_string(),
        };

        let function = self.module.get_function(&internal_name)
            .ok_or_else(|| format!("Undefined function: {}", name))?;

        let mut arg_values = Vec::new();
        for (i, arg) in arguments.iter().enumerate() {
            // Local array: pass pointer to first element instead of loading the array
            // Struct variable: pass pointer to the struct
            let special = match arg {
                Expression::Identifier(aname) => {
                    if let Some(&(arr_ptr, BasicTypeEnum::ArrayType(at))) = self.variables.get(aname.as_str()) {
                        Some({
                            let zero = self.context.i32_type().const_int(0, false);
                            let first = unsafe {
                                self.builder.build_gep(at, arr_ptr, &[zero, zero], "arr_arg")
                            }.unwrap();
                            BasicValueEnum::from(first)
                        })
                    } else if self.var_struct_names.contains_key(aname.as_str()) {
                        // Pass a pointer to the struct (its alloca)
                        let (alloca, _) = *self.variables.get(aname.as_str()).unwrap();
                        Some(BasicValueEnum::from(alloca))
                    } else {
                        None
                    }
                }
                Expression::FieldAccess { object, field } => {
                    if let Ok((struct_ptr, struct_name)) = self.resolve_struct_ptr(object) {
                        if let Some((st, field_names)) = self.struct_defs.get(&struct_name).cloned() {
                            if let Some(idx) = field_names.iter().position(|n| n == field) {
                                if let Some(BasicTypeEnum::StructType(_)) = st.get_field_type_at_index(idx as u32) {
                                    let field_ptr = self.builder
                                        .build_struct_gep(st, struct_ptr, idx as u32, "struct_field_arg")
                                        .unwrap();
                                    Some(BasicValueEnum::from(field_ptr))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let val = if let Some(v) = special {
                v
            } else {
                self.generate_expression(arg)?
            };

            // Auto-coerce i32 → i64 when the function signature expects i64
            let val = {
                let param_types = function.get_type().get_param_types();
                match (val, param_types.get(i)) {
                    (BasicValueEnum::IntValue(iv), Some(BasicTypeEnum::IntType(et))) => {
                        if iv.get_type().get_bit_width() < et.get_bit_width() {
                            self.builder.build_int_s_extend(iv, *et, "arg_widen").unwrap().into()
                        } else {
                            BasicValueEnum::IntValue(iv)
                        }
                    }
                    _ => val,
                }
            };

            arg_values.push(val.into());
        }

        let call_result = self.builder.build_call(
            function,
            &arg_values,
            &format!("{}_call", internal_name),
        ).unwrap();

        // Void functions have no return value — return i32 0 as dummy
        Ok(call_result.try_as_basic_value().left()
            .unwrap_or_else(|| self.context.i32_type().const_int(0, false).into()))
    }

    fn generate_binary_expr(
        &mut self,
        left: &Expression,
        op: &BinaryOp,
        right: &Expression,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let lhs = self.generate_expression(left)?;
        let rhs = self.generate_expression(right)?;

        match (lhs, rhs) {
            (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => {
                // LLVM requires same-width integer operands for arithmetic/comparisons.
                // Vit allows mixing i32/i64, so widen the narrower operand.
                let (l, r) = {
                    let lb = l.get_type().get_bit_width();
                    let rb = r.get_type().get_bit_width();
                    if lb == rb {
                        (l, r)
                    } else if lb < rb {
                        (
                            self.builder.build_int_s_extend(l, r.get_type(), "lhs_widen").unwrap(),
                            r,
                        )
                    } else {
                        (
                            l,
                            self.builder.build_int_s_extend(r, l.get_type(), "rhs_widen").unwrap(),
                        )
                    }
                };

                let result = match op {
                    BinaryOp::Add => self.builder.build_int_add(l, r, "add").unwrap(),
                    BinaryOp::Sub => self.builder.build_int_sub(l, r, "sub").unwrap(),
                    BinaryOp::Mul => self.builder.build_int_mul(l, r, "mul").unwrap(),
                    BinaryOp::Div => self.builder.build_int_signed_div(l, r, "div").unwrap(),
                    BinaryOp::Mod => self.builder.build_int_signed_rem(l, r, "rem").unwrap(),
                    BinaryOp::Equal        => self.builder.build_int_compare(IntPredicate::EQ,  l, r, "eq").unwrap(),
                    BinaryOp::NotEqual     => self.builder.build_int_compare(IntPredicate::NE,  l, r, "ne").unwrap(),
                    BinaryOp::Less         => self.builder.build_int_compare(IntPredicate::SLT, l, r, "lt").unwrap(),
                    BinaryOp::Greater      => self.builder.build_int_compare(IntPredicate::SGT, l, r, "gt").unwrap(),
                    BinaryOp::LessEqual    => self.builder.build_int_compare(IntPredicate::SLE, l, r, "le").unwrap(),
                    BinaryOp::GreaterEqual => self.builder.build_int_compare(IntPredicate::SGE, l, r, "ge").unwrap(),
                    BinaryOp::And    => self.builder.build_and(l, r, "and").unwrap(),
                    BinaryOp::Or     => self.builder.build_or(l, r, "or").unwrap(),
                    BinaryOp::BitAnd => self.builder.build_and(l, r, "band").unwrap(),
                    BinaryOp::BitOr  => self.builder.build_or(l, r, "bor").unwrap(),
                    BinaryOp::BitXor => self.builder.build_xor(l, r, "bxor").unwrap(),
                    BinaryOp::Shl    => self.builder.build_left_shift(l, r, "shl").unwrap(),
                    BinaryOp::Shr    => self.builder.build_right_shift(l, r, true, "shr").unwrap(),
                };
                Ok(result.into())
            }
            (BasicValueEnum::FloatValue(l), BasicValueEnum::FloatValue(r)) => {
                match op {
                    BinaryOp::Add => Ok(self.builder.build_float_add(l, r, "fadd").unwrap().into()),
                    BinaryOp::Sub => Ok(self.builder.build_float_sub(l, r, "fsub").unwrap().into()),
                    BinaryOp::Mul => Ok(self.builder.build_float_mul(l, r, "fmul").unwrap().into()),
                    BinaryOp::Div => Ok(self.builder.build_float_div(l, r, "fdiv").unwrap().into()),
                    BinaryOp::Mod => Err("Modulo não suportado para float".to_string()),
                    BinaryOp::Equal        => Ok(self.builder.build_float_compare(FloatPredicate::OEQ, l, r, "feq").unwrap().into()),
                    BinaryOp::NotEqual     => Ok(self.builder.build_float_compare(FloatPredicate::ONE, l, r, "fne").unwrap().into()),
                    BinaryOp::Less         => Ok(self.builder.build_float_compare(FloatPredicate::OLT, l, r, "flt").unwrap().into()),
                    BinaryOp::Greater      => Ok(self.builder.build_float_compare(FloatPredicate::OGT, l, r, "fgt").unwrap().into()),
                    BinaryOp::LessEqual    => Ok(self.builder.build_float_compare(FloatPredicate::OLE, l, r, "fle").unwrap().into()),
                    BinaryOp::GreaterEqual => Ok(self.builder.build_float_compare(FloatPredicate::OGE, l, r, "fge").unwrap().into()),
                    BinaryOp::And | BinaryOp::Or |
                    BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor |
                    BinaryOp::Shl | BinaryOp::Shr => Err("Operador não suportado para float".to_string()),
                }
            }
            _ => Err("Type mismatch in binary expression".to_string()),
        }
    }

    fn convert_type(&self, typ: &Type) -> BasicTypeEnum<'ctx> {
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

    fn build_fn_type(&self, return_type: BasicTypeEnum<'ctx>, params: &[BasicMetadataTypeEnum<'ctx>]) -> FunctionType<'ctx> {
        match return_type {
            BasicTypeEnum::IntType(t)    => t.fn_type(params, false),
            BasicTypeEnum::FloatType(t)  => t.fn_type(params, false),
            BasicTypeEnum::PointerType(t) => t.fn_type(params, false),
            BasicTypeEnum::StructType(t) => t.fn_type(params, false),
            _ => panic!("Functions cannot return this type"),
        }
    }

    // Auto-widen i32 → i64 when storing into wider type (keeps backward compat)
    fn coerce_int(&self, val: BasicValueEnum<'ctx>, target: BasicTypeEnum<'ctx>) -> BasicValueEnum<'ctx> {
        if let (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(tt)) = (val, target) {
            if iv.get_type().get_bit_width() < tt.get_bit_width() {
                return self.builder.build_int_s_extend(iv, tt, "widen").unwrap().into();
            }
        }
        val
    }

    fn block_terminated(&self) -> bool {
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
