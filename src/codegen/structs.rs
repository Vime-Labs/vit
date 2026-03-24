use crate::ast::*;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use super::Codegen;

impl<'ctx> Codegen<'ctx> {
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
                let resp_tmp = match resp {
                    BasicValueEnum::StructValue(sv) => {
                        let tmp = self.builder.build_alloca(response_type, "resp_tmp").unwrap();
                        self.builder.build_store(tmp, sv).unwrap();
                        Some(tmp)
                    }
                    _ => None,
                };
                let resp_arg_build: BasicMetadataValueEnum = match resp_tmp {
                    Some(tmp) => tmp.into(),
                    None => resp.into(),
                };
                let built = self.builder
                    .build_call(http_build_fn, &[resp_arg_build], "built_resp")
                    .unwrap()
                    .try_as_basic_value()
                    .left()
                    .ok_or_else(|| "http_handle(): http_build returned no value".to_string())?;
                let resp_arg_free: BasicMetadataValueEnum = match resp_tmp {
                    Some(tmp) => tmp.into(),
                    None => resp.into(),
                };
                self.builder.build_call(http_response_free_fn, &[resp_arg_free], "free_resp").unwrap();
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
}
