// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use cranelift_codegen::{
    isa,
    settings::{self, Configurable},
    Context,
};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{
    default_libcall_names, DataDescription, DataId, Linkage, Module, ModuleError,
};
use cranelift_object::{ObjectBuilder, ObjectModule};

// Documents of the Cranelift
//
// - home: https://cranelift.dev/
// - source code: https://github.com/bytecodealliance/wasmtime/tree/main/cranelift
// - docs: https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/index.md
// - IR Reference: https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/ir.md
// - InstBuilder: https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/trait.InstBuilder.html
// - Module: https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html
// - cranelift_frontend: https://docs.rs/cranelift-frontend/latest/cranelift_frontend/

pub struct Generator<T>
where
    T: Module,
{
    /// A `Module` is a utility for collecting functions and data objects, and linking them together.
    pub module: T,

    /// Structure used for translating a series of functions into Cranelift IR.
    ///
    /// In order to reduce memory reallocations when compiling multiple functions,
    /// [`FunctionBuilderContext`] holds various data structures which are cleared between
    /// functions, rather than dropped, preserving the underlying allocations.
    pub function_builder_context: FunctionBuilderContext,

    /// Allocate a new compilation context.
    ///
    /// The instance should be reused for compiling multiple functions in order to avoid
    /// needless allocator thrashing.
    context: Context,
}

impl Generator<JITModule> {
    // Documents of JITModule
    //
    // - source code: https://github.com/bytecodealliance/wasmtime/tree/main/cranelift/jit
    // - docs: https://docs.rs/cranelift-jit/latest/cranelift_jit/
    //
    // Demo:
    //
    // - https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/jit/examples/jit-minimal.rs
    // - https://github.com/bytecodealliance/cranelift-jit-demo/blob/main/src/jit.rs
    pub fn new(symbols: Vec<(String, *const u8)>) -> Self {
        // the building flow:
        //
        // flag builder -> isa builder -> jit builder -> jit module
        // All flags:
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html
        let mut flag_builder = settings::builder();

        // Use colocated libcalls.
        // Generate code that assumes that libcalls can be declared “colocated”,
        // meaning they will be defined along with the current function,
        // such that they can use more efficient addressing.
        //
        // - Traditional Libcalls: In traditional compilation, libcalls are external functions defined
        //   in a separate library. The compiler generates a call instruction that references
        //   the function's address.
        // - Colocated Libcalls:
        //   When this flag is set to true, Cranelift assumes that the libcall definitions are available
        //   within the same compilation unit. This allows the compiler to generate more efficient code by:
        //   - Direct Calls: Instead of generating a full call instruction, the compiler can use
        //   a direct jump to the libcall's address.
        //   - Reduced Relocation Overhead: By avoiding external references, the compiler can reduce
        //   the number of relocations needed, which can improve code size and loading time.
        // ref:
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.use_colocated_libcalls
        flag_builder.set("use_colocated_libcalls", "false").unwrap();

        // Enable Position-Independent Code generation.
        // ref:
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.is_pic
        flag_builder.set("is_pic", "true").unwrap();

        // Optimization level for generated code.
        //
        // Supported levels:
        //
        // none: Minimise compile time by disabling most optimizations.
        // speed: Generate the fastest possible code
        // speed_and_size: like “speed”, but also perform transformations aimed at reducing code size.
        // ref:
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.opt_level
        flag_builder.set("opt_level", "speed").unwrap();

        // Preserve frame pointers
        // Preserving frame pointers – even inside leaf functions – makes it easy to capture
        // the stack of a running program, without requiring any side tables or
        // metadata (like .eh_frame sections).
        // Many sampling profilers and similar tools walk frame pointers to capture stacks.
        // Enabling this option will play nice with those tools.
        // ref:
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.preserve_frame_pointers
        flag_builder.set("preserve_frame_pointers", "true").unwrap();

        // Defines the model used to perform TLS accesses.
        // note that the target "x86_64-unknown-linux-gnu" does not set "tls_model" by default.
        //
        // ref:
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.tls_model
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/enum.TlsModel.html
        //
        // possible values:
        //
        // - none
        // - elf_gd (ELF)
        // - macho (Mach-O)
        // - coff (COFF)
        flag_builder.set("tls_model", "none").unwrap();

        // Enable the use of atomic instructions
        // ref:
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.enable_atomics
        flag_builder.enable("enable_atomics").unwrap();

        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("The platform of the host machine is not supported: {}", msg);
        });

        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());

        // import external symbols
        //
        // to add single symbol:
        // `jit_builder.symbol(name:String, ptr:*const u8)`
        jit_builder.symbols(symbols);

        let module = JITModule::new(jit_builder);
        let context = module.make_context();
        let function_builder_context = FunctionBuilderContext::new();

        Self {
            module,
            context,
            function_builder_context,
        }
    }
}

impl Generator<ObjectModule> {
    // Documents of ObjectModule:
    //
    // - source code: https://github.com/bytecodealliance/wasmtime/tree/main/cranelift/object
    // - docs: https://docs.rs/cranelift-object/latest/cranelift_object/
    //
    // Demo:
    //
    // https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/object/tests/basic.rs
    pub fn new(module_name: &str, opt_platform: Option<&str>) -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.enable("is_pic").unwrap();
        flag_builder.set("opt_level", "none").unwrap();
        flag_builder.set("preserve_frame_pointers", "true").unwrap();
        flag_builder.set("tls_model", "elf_gd").unwrap();
        flag_builder.enable("enable_atomics").unwrap();

        let platform = opt_platform.unwrap_or("x86_64-unknown-linux-gnu");
        let isa_builder = isa::lookup_by_name(platform).unwrap_or_else(|msg| {
            panic!(
                "The target platform \"{}\" is not supported: {}",
                platform, msg
            );
        });

        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let object_builder = ObjectBuilder::new(isa, module_name, default_libcall_names()).unwrap();

        let module = ObjectModule::new(object_builder);
        let context = module.make_context();
        let function_builder_context = FunctionBuilderContext::new();

        Self {
            module,
            context,
            function_builder_context,
        }
    }
}

impl<T> Generator<T>
where
    T: Module,
{
    // https://docs.rs/cranelift-module/latest/cranelift_module/struct.DataDescription.html
    pub fn define_initialized_data(
        &mut self,
        name: &str,
        data: Vec<u8>,
        align: u64,
        export: bool,
        writable: bool,
        thread_local: bool,
    ) -> Result<DataId, ModuleError> {
        let linkage = if export {
            Linkage::Export
        } else {
            Linkage::Local
        };

        let mut data_description = DataDescription::new();
        data_description.define(data.into_boxed_slice());
        data_description.set_align(align);

        let data_id = self
            .module
            .declare_data(name, linkage, writable, thread_local)?;

        self.module.define_data(data_id, &data_description)?;

        Ok(data_id)
    }

    pub fn define_uninitialized_data(
        &mut self,
        name: &str,
        size: usize,
        align: u64,
        export: bool,
        thread_local: bool,
    ) -> Result<DataId, ModuleError> {
        let linkage = if export {
            Linkage::Export
        } else {
            Linkage::Local
        };

        let mut data_description = DataDescription::new();
        data_description.define_zeroinit(size);
        data_description.set_align(align);

        let data_id = self
            .module
            .declare_data(name, linkage, true, thread_local)?;
        self.module.define_data(data_id, &data_description)?;

        Ok(data_id)
    }
}

#[cfg(test)]
mod tests {
    use cranelift_codegen::ir::{
        types, AbiParam, Function, InstBuilder, StackSlotData, StackSlotKind, UserFuncName,
    };
    use cranelift_frontend::FunctionBuilder;
    use cranelift_jit::JITModule;
    use cranelift_module::{Linkage, Module};

    use crate::generator::Generator;

    #[test]
    fn test_jit_base() {
        // Some tips
        // ---------
        //
        // ## to get the pointer type (i32, i64 etc.):
        //
        // ```rust
        // let addr_t: Type = generator.module.isa().pointer_type();
        // ```
        //
        // ## to create a signature:
        //
        // ```rust
        // let sig_main = Signature {
        //     params: vec![],
        //     returns: vec![AbiParam::new(types::I32)],
        //     call_conv: CallConv::SystemV,
        // };
        // ```
        //
        // ## the calling convention:
        //
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/struct.Signature.html
        // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/isa/enum.CallConv.html
        //
        //
        // the name description:
        //
        // - fast         not-ABI-stable convention for best performance
        // - cold         not-ABI-stable convention for infrequently executed code
        // - system_v     System V-style convention used on many platforms
        // - fastcall     Windows "fastcall" convention, also used for x64 and ARM

        let mut generator = Generator::<JITModule>::new(vec![]);

        // build function "func_inc"
        //
        // ```rust
        // fn func_inc (a:i32) -> i32 {
        //    a+11
        // }
        // ```

        let mut func_inc_sig = generator.module.make_signature();
        func_inc_sig.params.push(AbiParam::new(types::I32));
        func_inc_sig.returns.push(AbiParam::new(types::I32));

        // the function "Module::declare_function()"
        // ref:
        // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#tymethod.declare_function
        let func_inc_declare = generator
            .module
            .declare_function("func_inc", Linkage::Export, &func_inc_sig)
            .unwrap();

        {
            // the following 'let mut func_inc = ...' and 'let mut function_builder = ...' is equivalent to:
            //
            // generator.context.func.signature = func_inc_sig;
            // generator.context.func.name = UserFuncName::user(0, func_inc_id.as_u32());
            //
            // let mut function_builder = FunctionBuilder::new(
            //     &mut generator.context.func,
            //     &mut function_builder_context,
            // );

            let mut func_inc = Function::with_name_signature(
                UserFuncName::user(0, func_inc_declare.as_u32()),
                func_inc_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_inc, &mut generator.function_builder_context);

            // the local variables
            // -------------------
            //
            // let x = Variable::new(0);
            // let y = Variable::new(1);
            // let z = Variable::new(2);
            // function_builder.declare_var(x, types::I32);
            // function_builder.declare_var(y, types::I32);
            // function_builder.declare_var(z, types::I32);
            // function_builder.def_var(x, tmp);        // set value
            // let .. = function_builder.use_var(x);    // get value
            //
            // ref:
            // - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/
            //
            // the stack slots
            // ---------------
            //
            // a sequence memory area in the stack, it is equivalent to
            // the XiaoXuan Core VM function's local variables area).
            //
            // func.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8));
            // function_builder.ins().stack_load(Mem, SS, Offset);
            // function_builder.ins().stack_store(x, SS, Offset);
            // function_builder.ins().stack_addr(iAddr, SS, Offset);
            //
            // ref:
            // - https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/trait.InstBuilder.html#method.stack_load

            let block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block);

            function_builder.switch_to_block(block);

            // the instructions:
            // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/trait.InstBuilder.html

            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.block_params(block)[0];
            let value_2 = function_builder.ins().iadd(value_0, value_1);
            function_builder.ins().return_(&[value_2]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // to display the text of IR
            // `println!("{}", func_inc.display());`

            // generate func_inc body's (machine/native) code

            generator.context.func = func_inc;

            generator
                .module
                .define_function(func_inc_declare, &mut generator.context)
                .unwrap();
        }

        // build function "func_main"
        //
        // ```rust
        // fn func_main () -> i32 {
        //    func_inc(13)
        // }
        // ```
        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        //
        // ref:
        // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#tymethod.declare_function
        let func_main_delcare = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_delcare.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            let block = function_builder.create_block();
            function_builder.switch_to_block(block);

            // ref:
            // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#method.declare_func_in_func
            let func_inc_ref = generator
                .module
                .declare_func_in_func(func_inc_declare, function_builder.func);

            let value0 = function_builder.ins().iconst(types::I32, 13);
            let call0 = function_builder.ins().call(func_inc_ref, &[value0]);
            let value1 = {
                let results = function_builder.inst_results(call0);
                assert_eq!(results.len(), 1);
                results[0]
            };
            function_builder.ins().return_(&[value1]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // generate func_main body's (machine/native) code

            generator.context.func = func_main;

            generator
                .module
                .define_function(func_main_delcare, &mut generator.context)
                .unwrap();
        }

        // all functions done
        generator.module.clear_context(&mut generator.context);

        // linking
        generator.module.finalize_definitions().unwrap();

        // get function pointers
        let func_inc_ptr = generator.module.get_finalized_function(func_inc_declare);
        let func_main_ptr = generator.module.get_finalized_function(func_main_delcare);

        // cast ptr to Rust function
        let func_inc: extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(func_inc_ptr) };
        let func_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(func_inc(0), 11);
        assert_eq!(func_inc(3), 14);
        assert_eq!(func_inc(13), 24);
        assert_eq!(func_main(), 24);
    }

    // for the following testing
    extern "C" fn add(a: i32, b: i32) -> i32 {
        a + b
    }

    // pass the address of external function through
    // the module symbol list.
    #[test]
    fn test_jit_call_external_function_by_importing_symbols() {
        // import/declare a function

        let func_add_ptr = add as *const u8; // as *const extern "C" fn(i32,i32)->i32;
        let symbols = vec![("func_add".to_owned(), func_add_ptr)];

        let mut generator = Generator::<JITModule>::new(symbols);

        let mut func_add_sig = generator.module.make_signature();
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.returns.push(AbiParam::new(types::I32));

        let func_add_declare = generator
            .module
            .declare_function("func_add", Linkage::Import, &func_add_sig)
            .unwrap();

        // build function "main"
        //
        // ```rust
        // extern "C" fn add(i32, i32) -> i32;
        // fn main() -> int {
        //     add(11, 13)
        // }
        // ```

        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        let func_main_declare = generator
            .module
            .declare_function("main", Linkage::Local, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_declare.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            // ref:
            // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#method.declare_func_in_func
            let func_add_ref = generator
                .module
                .declare_func_in_func(func_add_declare, function_builder.func);

            let block_0 = function_builder.create_block();
            function_builder.switch_to_block(block_0);

            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.ins().iconst(types::I32, 13);
            let call0 = function_builder
                .ins()
                .call(func_add_ref, &[value_0, value_1]);
            let value_2 = function_builder.inst_results(call0)[0];

            function_builder.ins().return_(&[value_2]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // to display the text of IR
            // `println!("{}", func_main.display());`

            // generate the (machine/native) code of func_main
            generator.context.func = func_main;

            generator
                .module
                .define_function(func_main_declare, &mut generator.context)
                .unwrap();
        }

        // all functions done
        generator.module.clear_context(&mut generator.context);

        // link
        generator.module.finalize_definitions().unwrap();

        // get func_main ptr
        let func_main_ptr = generator.module.get_finalized_function(func_main_declare);
        let func_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        // call func_main
        assert_eq!(func_main(), 24);
    }

    // pass the address of external function through
    // the function argument, and call the target function
    // by IR 'call_indirect' instruction.
    #[test]
    fn test_jit_call_external_function_by_function_address() {
        let mut generator = Generator::<JITModule>::new(vec![]);
        let pointer_type = generator.module.isa().pointer_type();

        let mut func_add_sig = generator.module.make_signature();
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.returns.push(AbiParam::new(types::I32));

        // build function "callme"
        //
        // fn callme(func_add: *const extern "C" fn(i32,i32)->i32) -> int {
        //     (func_add)(11, 13) /* IR: call_indirect(func_add, 11, 13) */
        // }

        let mut func_callme_sig = generator.module.make_signature();
        func_callme_sig.params.push(AbiParam::new(pointer_type));
        func_callme_sig.returns.push(AbiParam::new(types::I32));

        let func_callme_declare = generator
            .module
            .declare_function("callme", Linkage::Local, &func_callme_sig)
            .unwrap();

        {
            let mut func_callme = Function::with_name_signature(
                UserFuncName::user(0, func_callme_declare.as_u32()),
                func_callme_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_callme, &mut generator.function_builder_context);

            let block_0 = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_0);
            function_builder.switch_to_block(block_0);

            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.ins().iconst(types::I32, 13);
            let value_2 = function_builder.block_params(block_0)[0];

            let func_add_sig_ref = function_builder.import_signature(func_add_sig);

            let call0 = function_builder.ins().call_indirect(
                func_add_sig_ref,
                value_2,
                &[value_0, value_1],
            );
            let value_2 = function_builder.inst_results(call0)[0];

            function_builder.ins().return_(&[value_2]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // generate the (machine/native) code of func_main
            generator.context.func = func_callme;

            generator
                .module
                .define_function(func_callme_declare, &mut generator.context)
                .unwrap();
        }

        // all functions done
        generator.module.clear_context(&mut generator.context);

        // link
        generator.module.finalize_definitions().unwrap();

        // get func_main ptr
        let func_callme_ptr = generator.module.get_finalized_function(func_callme_declare);
        let func_callme: extern "C" fn(usize) -> i32 =
            unsafe { std::mem::transmute(func_callme_ptr) };

        // call func_main
        let func_add_addr = add as *const u8 as usize;
        assert_eq!(func_callme(func_add_addr), 24);
    }

    // for the following testing
    extern "C" fn check_byte_array(foo: *const u8, bar: *const u8) {
        // foo:
        // | 8 bytes | 8 bytes | 8 bytes | 8 bytes | 8 bytes |
        // | i32     | i64     | f32     | f64     | usize   |
        //                                              |
        //                                              \--> a pointer which point to `[i32; 2]`
        //
        // bar:
        // | 8 bytes |
        // | i32     |

        let foo_i = unsafe { std::ptr::read(foo.add(0) as *const i32) };
        let foo_j = unsafe { std::ptr::read(foo.add(8) as *const i64) };
        let foo_m = unsafe { std::ptr::read(foo.add(16) as *const f32) };
        let foo_n = unsafe { std::ptr::read(foo.add(24) as *const f64) };
        let foo_p = unsafe { std::ptr::read(foo.add(32) as *const usize) };

        // write '211' to the 'bar' when values of all members of "foo" are as expected,
        // otherwise write '199'

        let pass = (foo_i == 41) && (foo_j == 43) && (foo_m == 3.5) && (foo_n == 7.5);
        let result_value = if pass { 211 } else { 199 };
        unsafe { std::ptr::write(bar as *mut i32, result_value) };

        // `foo_ptr` is a pointer which point to an i32 array `[i32; 2]`:
        // | 4 bytes | 4 bytes |
        // | i32     | i32     |

        // write '109' and '113' to the i32 array
        let foo_p_array = unsafe { std::slice::from_raw_parts_mut(foo_p as *mut i32, 2) };
        foo_p_array[0] = 53;
        foo_p_array[1] = 59;
    }

    /// this testing will call an external function with byte array pointer parameters
    #[test]
    fn test_jit_local_variables() {
        let mut generator = Generator::<JITModule>::new(vec![]);
        let pointer_type = generator.module.isa().pointer_type();

        let func_check_byte_array_addr = check_byte_array as *const u8 as usize;

        let mut func_check_byte_array_sig = generator.module.make_signature();
        func_check_byte_array_sig
            .params
            .push(AbiParam::new(pointer_type));
        func_check_byte_array_sig
            .params
            .push(AbiParam::new(pointer_type));

        // build function "func_callme"
        //
        // fn callme(i32, i64, f32, f64, &[i32; 2]) -> i32 {
        //      let foo:[u8; 40] = .. // store values of params
        //      let bar:[u8; 8] = .. // init with zero
        //      check_byte_array(&mut foo, &mut bar)
        //      let result = // read `bar` as i32
        //      return result;
        // }

        let mut func_callme_sig = generator.module.make_signature();
        func_callme_sig.params.push(AbiParam::new(types::I32));
        func_callme_sig.params.push(AbiParam::new(types::I64));
        func_callme_sig.params.push(AbiParam::new(types::F32));
        func_callme_sig.params.push(AbiParam::new(types::F64));
        func_callme_sig.params.push(AbiParam::new(pointer_type));
        func_callme_sig.returns.push(AbiParam::new(types::I32));

        // the IR of func_callme:
        //
        // ```ir
        // function u0:0(i32, i64, f32, f64, i64) -> i32 system_v {
        //     ss0 = explicit_slot 40
        //     ss1 = explicit_slot 8
        //     sig0 = (i64, i64) system_v
        //
        // block0(v0: i32, v1: i64, v2: f32, v3: f64, v4: i64):
        //     stack_store v0, ss0
        //     stack_store v1, ss0+8
        //     stack_store v2, ss0+16
        //     stack_store v3, ss0+24
        //     stack_store v4, ss0+32
        //     v5 = iconst.i64 0x559f_1144_8df0
        //     v6 = stack_addr.i64 ss0
        //     v7 = stack_addr.i64 ss1
        //     call_indirect sig0, v5(v6, v7)
        //     v8 = stack_load.i32 ss1
        //     return v8
        // }
        // ```

        let func_callme_declare = generator
            .module
            .declare_function("callme", Linkage::Local, &func_callme_sig)
            .unwrap();

        {
            let mut func_callme = Function::with_name_signature(
                UserFuncName::user(0, func_callme_declare.as_u32()),
                func_callme_sig,
            );

            // create two stack slots
            let ss0 = func_callme.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                40,
                2,
            ));

            let ss1 = func_callme.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                2,
            ));

            let mut function_builder =
                FunctionBuilder::new(&mut func_callme, &mut generator.function_builder_context);

            let block_0 = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_0);
            function_builder.switch_to_block(block_0);

            let value_0 = function_builder.block_params(block_0)[0];
            let value_1 = function_builder.block_params(block_0)[1];
            let value_2 = function_builder.block_params(block_0)[2];
            let value_3 = function_builder.block_params(block_0)[3];
            let value_4 = function_builder.block_params(block_0)[4];

            function_builder.ins().stack_store(value_0, ss0, 0);
            function_builder.ins().stack_store(value_1, ss0, 8);
            function_builder.ins().stack_store(value_2, ss0, 16);
            function_builder.ins().stack_store(value_3, ss0, 24);
            function_builder.ins().stack_store(value_4, ss0, 32);

            let addr_0 = function_builder
                .ins()
                .iconst(pointer_type, func_check_byte_array_addr as i64);
            let ptr_0 = function_builder.ins().stack_addr(pointer_type, ss0, 0);
            let ptr_1 = function_builder.ins().stack_addr(pointer_type, ss1, 0);

            let func_check_byte_array_sig_ref =
                function_builder.import_signature(func_check_byte_array_sig);
            function_builder.ins().call_indirect(
                func_check_byte_array_sig_ref,
                addr_0,
                &[ptr_0, ptr_1],
            );

            let value_ret = function_builder.ins().stack_load(types::I32, ss1, 0);

            function_builder.ins().return_(&[value_ret]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // println!("{}", func_main.display());

            // generate the (machine/native) code of func_main
            generator.context.func = func_callme;

            generator
                .module
                .define_function(func_callme_declare, &mut generator.context)
                .unwrap();
        }

        // all functions done
        generator.module.clear_context(&mut generator.context);

        // link
        generator.module.finalize_definitions().unwrap();

        // get "func_callme" ptr
        let func_callme_ptr = generator.module.get_finalized_function(func_callme_declare);
        let func_callme: extern "C" fn(i32, i64, f32, f64, usize) -> i32 =
            unsafe { std::mem::transmute(func_callme_ptr) };

        // construct the 5th argument of the function "func_callme"
        let buf: [u8; 8] = [31, 0, 0, 0, 37, 0, 0, 0];
        let buf_addr = buf.as_ptr() as usize;

        // call "func_callme"
        assert_eq!(func_callme(41, 43, 3.5, 7.5, buf_addr), 211);

        let buf_as_i32x2 = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const i32, 2) };
        assert_eq!(buf_as_i32x2[0], 53);
        assert_eq!(buf_as_i32x2[1], 59);
    }
}
