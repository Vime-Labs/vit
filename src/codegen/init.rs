use crate::ast::*;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use super::Codegen;

impl<'ctx> Codegen<'ctx> {
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
            // Null-terminate so strbuf_to_str() is safe on an empty buffer
            self.builder.build_store(data, i8_type.const_int(0, false)).unwrap();

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
}
