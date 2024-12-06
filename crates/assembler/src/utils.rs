// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.


#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        io::Write,
        process::{Command, ExitStatus},
    };

    use cranelift_codegen::ir::{
        condcodes::IntCC,
        immediates::Offset32,
        types::{self},
        AbiParam, Function, InstBuilder, MemFlags, StackSlotData, StackSlotKind, Type,
        UserFuncName,
    };
    use cranelift_frontend::FunctionBuilder;
    use cranelift_jit::JITModule;
    use cranelift_module::{Linkage, Module};
    use cranelift_object::ObjectModule;

    #[test]
    fn test_jit_base() {
        let mut generator = Generator::<JITModule>::new(vec![]);

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

        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        //
        // ref:
        // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#tymethod.declare_function
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let mut func_builder: FunctionBuilder = FunctionBuilder::new(
                // &mut generator.context.func,
                &mut func_main,
                &mut generator.function_builder_context,
            );
            let block = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block);
            func_builder.switch_to_block(block);

            // return const 11
            let value_0 = func_builder.ins().iconst(types::I32, 11);
            func_builder.ins().return_(&[value_0]);

            func_builder.seal_all_blocks();
            func_builder.finalize();

            // generate the function code

            generator.context.func = func_main;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        generator.module.finalize_definitions().unwrap();

        // get function pointers
        let func_main_ptr = generator.module.get_finalized_function(func_main_id);

        // cast ptr to Rust function
        let func_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(func_main(), 11);
    }

    #[test]
    fn test_utils_object_file() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let mut func_builder: FunctionBuilder = FunctionBuilder::new(
                // &mut generator.context.func,
                &mut func,
                &mut generator.function_builder_context,
            );
            let block = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block);
            func_builder.switch_to_block(block);

            // return const 11
            let value_0 = func_builder.ins().iconst(types::I32, 11);
            func_builder.ins().return_(&[value_0]);

            func_builder.seal_all_blocks();
            func_builder.finalize();

            // generate the function code

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // ObjectProduct:
        // https://docs.rs/cranelift-object/latest/cranelift_object/struct.ObjectProduct.html

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code(
            &module_binary,
            "anna_unit_test_utils_object_file",
        );

        assert_eq!(exit_code_opt, Some(11));
    }

    fn get_temp_file_path(filename: &str) -> String {
        let mut dir = std::env::temp_dir();
        dir.push(filename);
        dir.to_str().unwrap().to_owned()
    }

    fn link_object_file(
        object_file: &str,
        lib_path: Option<&str>,
        lib_soname: Option<&str>,
        output_file: &str,
    ) -> std::io::Result<ExitStatus> {
        // link the object file with GCC:
        //
        // `$ gcc -o anna.elf anna.o`
        //
        // link the object file with binutils 'ld':
        //
        // ```sh
        // ld \
        //     -dynamic-linker /lib64/ld-linux-x86-64.so.2 \
        //     -pie \
        //     -o anna.elf \
        //     /usr/lib/Scrt1.o \
        //     /usr/lib/crti.o \
        //     -L/lib/ \
        //     -L/usr/lib \
        //     anna.o \
        //     -lc \
        //     /usr/lib/crtn.o
        // ```
        //
        // reference: the result of command `$ gcc -v -o anna.elf anna.o`

        // Mini FAQ about the misc libc/gcc crt files.
        // https://dev.gentoo.org/~vapier/crt.txt
        //
        // Some definitions:
        // - PIC - position independent code (-fPIC)
        // - PIE - position independent executable (-fPIE -pie)
        // - crt - C runtime
        //
        // - crt0.o crt1.o etc...
        //   Some systems use crt0.o, while some use crt1.o (and a few even use crt2.o
        //   or higher).  Most likely due to a transitionary phase that some targets
        //   went through.  The specific number is otherwise entirely arbitrary -- look
        //   at the internal gcc port code to figure out what your target expects.  All
        //   that matters is that whatever gcc has encoded, your C library better use
        //   the same name.
        //
        //   This object is expected to contain the _start symbol which takes care of
        //   bootstrapping the initial execution of the program.  What exactly that
        //   entails is highly libc dependent and as such, the object is provided by
        //   the C library and cannot be mixed with other ones.
        //
        //   On uClibc/glibc systems, this object initializes very early ABI requirements
        //   (like the stack or frame pointer), setting up the argc/argv/env values, and
        //   then passing pointers to the init/fini/main funcs to the internal libc main
        //   which in turn does more general bootstrapping before finally calling the real
        //   main function.
        //
        //   glibc ports call this file 'start.S' while uClibc ports call this crt0.S or
        //   crt1.S (depending on what their gcc expects).
        //
        // - crti.o
        //   Defines the function prologs for the .init and .fini sections (with the _init
        //   and _fini symbols respectively).  This way they can be called directly.  These
        //   symbols also trigger the linker to generate DT_INIT/DT_FINI dynamic ELF tags.
        //
        //   These are to support the old style constructor/destructor system where all
        //   .init/.fini sections get concatenated at link time.  Not to be confused with
        //   newer prioritized constructor/destructor .init_array/.fini_array sections and
        //   DT_INIT_ARRAY/DT_FINI_ARRAY ELF tags.
        //
        //   glibc ports used to call this 'initfini.c', but now use 'crti.S'.  uClibc
        //   also uses 'crti.S'.
        //
        // - crtn.o
        //   Defines the function epilogs for the .init/.fini sections.  See crti.o.
        //
        //   glibc ports used to call this 'initfini.c', but now use 'crtn.S'.  uClibc
        //   also uses 'crtn.S'.
        //
        // - Scrt1.o
        //   Used in place of crt1.o when generating PIEs.
        // - gcrt1.o
        //   Used in place of crt1.o when generating code with profiling information.
        //   Compile with -pg.  Produces output suitable for the gprof util.
        // - Mcrt1.o
        //   Like gcrt1.o, but is used with the prof utility.  glibc installs this as
        //   a dummy file as it's useless on linux systems.
        //
        // - crtbegin.o
        //   GCC uses this to find the start of the constructors.
        // - crtbeginS.o
        //   Used in place of crtbegin.o when generating shared objects/PIEs.
        // - crtbeginT.o
        //   Used in place of crtbegin.o when generating static executables.
        // - crtend.o
        //   GCC uses this to find the start of the destructors.
        // - crtendS.o
        //   Used in place of crtend.o when generating shared objects/PIEs.
        //
        // General linking order:
        // ```
        // crt1.o crti.o crtbegin.o
        //     [-L paths] [user objects] [gcc libs] [C libs] [gcc libs]
        //     crtend.o crtn.o
        // ```
        //
        // More references:
        // - http://gcc.gnu.org/onlinedocs/gccint/Initialization.html
        // - https://stackoverflow.com/a/16436294/23069938
        //
        // file 'Scrt1.o' is owned by package 'glibc', check:
        // `$ pacman -Qo Scrt1.o`
        // `$ pacman -Ql glibc | grep crt`

        let mut args = vec![];

        args.push("--dynamic-linker");
        args.push("/lib64/ld-linux-x86-64.so.2");
        args.push("-pie");
        args.push("-o");
        args.push(output_file);
        args.push("/usr/lib/Scrt1.o");
        args.push("/usr/lib/crti.o");
        args.push("-L/lib/");
        args.push("-L/usr/lib");

        if let Some(lib_path_str) = lib_path {
            args.push("-L");
            args.push(lib_path_str);
        }

        args.push(object_file);

        if let Some(lib_soname_str) = lib_soname {
            args.push("-l");
            args.push(lib_soname_str);
        }

        args.push("-lc");
        args.push("/usr/lib/crtn.o");

        Command::new("/usr/bin/ld").args(args).status()
    }

    fn delete_file(filepath: &str) {
        std::fs::remove_file(filepath).unwrap();
    }

    fn get_userlib_path() -> String {
        let mut pwd = std::env::current_dir().unwrap();

        // let pkg_name = env!("CARGO_PKG_NAME");       // the name of package that is written within the file 'Cargo.toml'
        // let crate_name = env!("CARGO_CRATE_NAME");  // the name of package which is convert '-' to '_'.
        // ref:
        // https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates
        let crate_dir_name = env!("CARGO_MANIFEST_DIR");
        if !pwd.ends_with(crate_dir_name) {
            // in the VSCode editor `Debug` environment, the `current_dir()` returns
            // the project's root folder.
            // while in both `$ cargo test` and VSCode editor `Run Test` environment,
            // the `current_dir()` returns the current crate path.
            // here canonicalize the test resources path.
            pwd.push("crates");
            pwd.push(crate_dir_name);
        }

        pwd.push("tests");
        pwd.push("lib");
        pwd.to_str().unwrap().to_string()
    }

    fn run_executable_binary_and_get_exit_code(module_binary: &[u8], name: &str) -> Option<i32> {
        // write object file
        let object_file_path = get_temp_file_path(&format!("{}.o", name));
        let mut file = File::create(&object_file_path).unwrap();
        file.write_all(&module_binary).unwrap();

        // link file
        let exec_file_path = get_temp_file_path(&format!("{}.elf", name));
        link_object_file(&object_file_path, None, None, &exec_file_path).unwrap();

        // Run the executable file and get the exit code
        // `$ ./anna.elf`
        // `$ echo $?`

        // run executable file and get exit code
        let exit_code_opt = Command::new(&exec_file_path).status().unwrap().code();

        // clean up
        delete_file(&object_file_path);
        delete_file(&exec_file_path);

        exit_code_opt
    }

    fn run_executable_binary_and_get_exit_code_with_libtest0(
        module_binary: &[u8],
        name: &str,
    ) -> Option<i32> {
        // write object file
        let object_file_path = get_temp_file_path(&format!("{}.o", name));
        let mut file = File::create(&object_file_path).unwrap();
        file.write_all(&module_binary).unwrap();

        // link file
        let user_lib_path = get_userlib_path();
        let user_lib_soname = "test0";
        let exec_file_path = get_temp_file_path(&format!("{}.elf", name));

        println!("user lib path: {}", user_lib_path);

        link_object_file(
            &object_file_path,
            Some(&user_lib_path),
            Some(user_lib_soname),
            &exec_file_path,
        )
        .unwrap();

        // run executable file and get exit code
        let exit_code_opt = Command::new(&exec_file_path)
            .env("LD_LIBRARY_PATH", &user_lib_path)
            .status()
            .unwrap()
            .code();

        // clean up
        delete_file(&object_file_path);
        delete_file(&exec_file_path);

        exit_code_opt
    }

    #[test]
    fn test_utils_function_call() {
        /*
        function u0:1() -> i32 system_v {
            sig0 = (i32, i32) -> i32, i32 system_v
            fn0 = colocated u0:0 sig0

        block0:
            v1 = iconst.i32 11
            v2 = iconst.i32 13
            v3, v4 = call fn0(v1, v2)  ; v1 = 11, v2 = 13
            jump block1

        block1:
            v5 = icmp_imm.i32 eq v3, 13
            v6 = iconst.i32 1
            brif v5, block2, block3(v6)  ; v6 = 1

        block2:
            v7 = icmp_imm.i32 eq v4, 11
            v8 = iconst.i32 2
            v9 = iconst.i32 0
            brif v7, block3(v9), block3(v8)  ; v9 = 0, v8 = 2

        block3(v0: i32):
            return v0
        }
         */
        let mut generator = Generator::new_object_file("main");

        let mut sig_swap = generator.module.make_signature();
        sig_swap.params.push(AbiParam::new(types::I32));
        sig_swap.params.push(AbiParam::new(types::I32));
        sig_swap.returns.push(AbiParam::new(types::I32));
        sig_swap.returns.push(AbiParam::new(types::I32));

        let func_swap_id = generator
            .module
            .declare_function("swap", Linkage::Local, &sig_swap)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_swap_id.as_u32()),
                sig_swap,
            );

            let mut func_builder: FunctionBuilder = FunctionBuilder::new(
                // &mut generator.context.func,
                &mut func,
                &mut generator.function_builder_context,
            );
            let block = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block);
            func_builder.switch_to_block(block);

            let value_a = func_builder.block_params(block)[0];
            let value_b = func_builder.block_params(block)[1];

            // return (b, a)
            func_builder.ins().return_(&[value_b, value_a]);

            func_builder.seal_all_blocks();
            func_builder.finalize();

            // generate the function code

            generator.context.func = func;

            generator
                .module
                .define_function(func_swap_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let func_ref0 = generator
                .module
                .declare_func_in_func(func_swap_id, &mut func);

            let mut func_builder: FunctionBuilder = FunctionBuilder::new(
                // &mut generator.context.func,
                &mut func,
                &mut generator.function_builder_context,
            );

            // ()                                 (i32)
            // start ---> check0 ---> check1 ---> exit
            //                    |           ^
            //                    \-----------/

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_check0 = func_builder.create_block();
            let block_check1 = func_builder.create_block();

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);

            // call swap(11, 13)
            // results == (13, 11)
            let value_0 = func_builder.ins().iconst(types::I32, 11);
            let value_1 = func_builder.ins().iconst(types::I32, 13);

            let call0 = func_builder.ins().call(func_ref0, &[value_0, value_1]);
            let call0_results = func_builder.inst_results(call0).to_vec();
            func_builder.ins().jump(block_check0, &[]);

            // build block_check0
            // assert results[0] == 13
            func_builder.switch_to_block(block_check0);

            // check results 1/2
            let check_result_0 = func_builder
                .ins()
                .icmp_imm(IntCC::Equal, call0_results[0], 13);
            let exit_code_imm_1 = func_builder.ins().iconst(types::I32, 1);

            func_builder.ins().brif(
                check_result_0,
                block_check1,
                &[],
                block_exit,
                &[exit_code_imm_1],
            );

            // build block_check1
            func_builder.switch_to_block(block_check1);

            // check results 2/2
            // assert results[1] == 11
            let check_result_1 = func_builder
                .ins()
                .icmp_imm(IntCC::Equal, call0_results[1], 11);
            let exit_code_imm_2 = func_builder.ins().iconst(types::I32, 2);
            let exit_code_imm_0 = func_builder.ins().iconst(types::I32, 0);

            func_builder.ins().brif(
                check_result_1,
                block_exit,
                &[exit_code_imm_0],
                block_exit,
                &[exit_code_imm_2],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            // generate the function code

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code(
            &module_binary,
            "anna_unit_test_utils_function_call",
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_utils_data() {
        /*
        function u0:0() -> i32 system_v {
            gv0 = symbol colocated userextname0
            gv1 = symbol colocated userextname1

        block0:
            jump block1

        block1:
            v1 = symbol_value.i64 gv0
            v2 = load.i32 v1
            v3 = icmp_imm eq v2, 11
            v4 = iconst.i32 1
            brif v3, block2, block4(v4)  ; v4 = 1

        block2:
            v5 = symbol_value.i64 gv1
            v6 = load.i32 v5
            v7 = icmp_imm eq v6, 13
            v8 = iconst.i32 2
            brif v7, block3, block4(v8)  ; v8 = 2

        block3:
            v9 = iconst.i32 17
            store v9, v5  ; v9 = 17
            v10 = load.i32 v5
            v11 = icmp_imm eq v10, 17
            v12 = iconst.i32 0
            v13 = iconst.i32 3
            brif v11, block4(v12), block4(v13)  ; v12 = 0, v13 = 3

        block4(v0: i32):
            return v0
        }
         */
        let mut generator = Generator::new_object_file("main");

        let addr_t: Type = generator.module.isa().pointer_type();

        // define read-only data
        let data_ro_content = 11u32.to_le_bytes().to_vec();
        let data_ro_id = generator
            .define_inited_data("number0", data_ro_content, 4, Linkage::Local, false, false)
            .unwrap();

        // define read-write data
        let data_rw_content = 13u32.to_le_bytes().to_vec();
        let data_rw_id = generator
            .define_inited_data("number1", data_rw_content, 4, Linkage::Local, true, false)
            .unwrap();

        // define function
        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let gv_data_ro = generator.module.declare_data_in_func(data_ro_id, &mut func);
            let gv_data_rw = generator.module.declare_data_in_func(data_rw_id, &mut func);

            let mut func_builder: FunctionBuilder = FunctionBuilder::new(
                // &mut generator.context.func,
                &mut func,
                &mut generator.function_builder_context,
            );

            //            check ro    check rw    update and check rw
            // start ---> check0 ---> check1 ---> check2  ---> exit
            //                    |           |            ^
            //                    |           \------------|
            //                    \------------------------/

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_check0 = func_builder.create_block();
            let block_check1 = func_builder.create_block();
            let block_check2 = func_builder.create_block();

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);
            func_builder.ins().jump(block_check0, &[]);

            // build block_check0
            // assert data0 == 11
            func_builder.switch_to_block(block_check0);
            let data_ro_addr = func_builder.ins().symbol_value(addr_t, gv_data_ro);
            let value_ro_0 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), data_ro_addr, 0);

            let check_result_0 = func_builder.ins().icmp_imm(IntCC::Equal, value_ro_0, 11);
            let exit_code_imm_1 = func_builder.ins().iconst(types::I32, 1);

            func_builder.ins().brif(
                check_result_0,
                block_check1,
                &[],
                block_exit,
                &[exit_code_imm_1],
            );

            // build block_check1
            // assert data1 == 13
            func_builder.switch_to_block(block_check1);
            let data_rw_addr = func_builder.ins().symbol_value(addr_t, gv_data_rw);
            let value_rw_0 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), data_rw_addr, 0);

            let check_result_1 = func_builder.ins().icmp_imm(IntCC::Equal, value_rw_0, 13);
            let exit_code_imm_2 = func_builder.ins().iconst(types::I32, 2);

            func_builder.ins().brif(
                check_result_1,
                block_check2,
                &[],
                block_exit,
                &[exit_code_imm_2],
            );

            // build block_check2
            // write 17 to data1, and read and assert it is 17
            func_builder.switch_to_block(block_check2);
            let value_imm_17 = func_builder.ins().iconst(types::I32, 17);
            func_builder
                .ins()
                .store(MemFlags::new(), value_imm_17, data_rw_addr, 0);

            let value_rw_1 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), data_rw_addr, 0);

            let check_result_2 = func_builder.ins().icmp_imm(IntCC::Equal, value_rw_1, 17);
            let exit_code_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let exit_code_imm_3 = func_builder.ins().iconst(types::I32, 3);

            func_builder.ins().brif(
                check_result_2,
                block_exit,
                &[exit_code_imm_0],
                block_exit,
                &[exit_code_imm_3],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // note:
        // the flow for JIT module:
        //
        // 1.linking
        // `generator.module.finalize_definitions().unwrap();`
        //
        // 2. get function pointers
        // `let func_main_ptr = generator.module.get_finalized_function(func_main_id);`
        //
        // 3. get data pointer
        //
        // ```rust
        // let (buf_ptr, buf_size) = generator.module.get_finalized_data(data_id);
        // let buf = unsafe { std::slice::from_raw_parts(buf_ptr, buf_size) };
        // ```
        //
        // note that the pointers of functions and data only available after 'module.finalize_definitions()'
        //
        // 4. cast ptr to Rust function
        // `let func_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };`
        //
        // 5. execute the function:
        // `assert_eq!(func_main(), 13);`

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt =
            run_executable_binary_and_get_exit_code(&module_binary, "anna_unit_test_utils_data");

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_utils_local_variable() {
        // 'local variable' = 'data that allocated on the stack'

        /*
        function u0:0() -> i32 system_v {
            ss0 = explicit_slot 4

        block0:
            jump block1

        block1:
            v1 = iconst.i32 11
            stack_store v1, ss0  ; v1 = 11
            v2 = stack_load.i32 ss0
            v3 = icmp_imm eq v2, 11
            v4 = iconst.i32 1
            brif v3, block2, block4(v4)  ; v4 = 1

        block2:
            v5 = stack_addr.i64 ss0
            v6 = load.i32 v5
            v7 = icmp_imm eq v6, 11
            v8 = iconst.i32 2
            brif v7, block3, block4(v8)  ; v8 = 2

        block3:
            v9 = iconst.i32 17
            store v9, v5  ; v9 = 17
            v10 = stack_load.i32 ss0
            v11 = icmp_imm eq v10, 17
            v12 = iconst.i32 0
            v13 = iconst.i32 3
            brif v11, block4(v12), block4(v13)  ; v12 = 0, v13 = 3

        block4(v0: i32):
            return v0
        }
         */

        let mut generator = Generator::new_object_file("main");

        let addr_t: Type = generator.module.isa().pointer_type();

        // define function
        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let ss0 =
                func.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4));

            let mut func_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func, &mut generator.function_builder_context);

            //            check ss0   load ss0    store ss0
            //                        by mem.load by mem.store
            // start ---> check0 ---> check1 ---> check2 ---> exit
            //                    |           |           ^
            //                    |           \-----------|
            //                    \-----------------------/

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_check0 = func_builder.create_block();
            let block_check1 = func_builder.create_block();
            let block_check2 = func_builder.create_block();

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);
            func_builder.ins().jump(block_check0, &[]);

            // build block_check0
            func_builder.switch_to_block(block_check0);

            let value_imm_11 = func_builder.ins().iconst(types::I32, 11);
            func_builder
                .ins()
                .stack_store(value_imm_11, ss0, Offset32::new(0));

            let value_0 = func_builder.ins().stack_load(types::I32, ss0, 0);

            let check_result_0 = func_builder.ins().icmp_imm(IntCC::Equal, value_0, 11);
            let exit_code_imm_1 = func_builder.ins().iconst(types::I32, 1);

            func_builder.ins().brif(
                check_result_0,
                block_check1,
                &[],
                block_exit,
                &[exit_code_imm_1],
            );

            // build block_check1
            func_builder.switch_to_block(block_check1);
            let local_var_addr = func_builder.ins().stack_addr(addr_t, ss0, 0);
            let value_1 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), local_var_addr, 0);

            let check_result_1 = func_builder.ins().icmp_imm(IntCC::Equal, value_1, 11);
            let exit_code_imm_2 = func_builder.ins().iconst(types::I32, 2);

            func_builder.ins().brif(
                check_result_1,
                block_check2,
                &[],
                block_exit,
                &[exit_code_imm_2],
            );

            // build block_check2
            func_builder.switch_to_block(block_check2);
            let value_imm_17 = func_builder.ins().iconst(types::I32, 17);
            func_builder
                .ins()
                .store(MemFlags::new(), value_imm_17, local_var_addr, 0);

            let value_2 = func_builder.ins().stack_load(types::I32, ss0, 0);

            let check_result_2 = func_builder.ins().icmp_imm(IntCC::Equal, value_2, 17);
            let exit_code_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let exit_code_imm_3 = func_builder.ins().iconst(types::I32, 3);

            func_builder.ins().brif(
                check_result_2,
                block_exit,
                &[exit_code_imm_0],
                block_exit,
                &[exit_code_imm_3],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code(
            &module_binary,
            "anna_unit_test_utils_local_variable",
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_utils_control_flow() {
        /*
        function u0:0() -> i32 system_v {
        block0:
            v4 = iconst.i32 0
            v5 = iconst.i32 10
            jump block1(v4, v5)  ; v4 = 0, v5 = 10

        block1(v0: i32, v1: i32):
            v6 = iadd v0, v1
            v7 = iadd_imm v1, -1
            v8 = icmp_imm eq v7, 0
            brif v8, block2(v6), block1(v6, v7)

        block2(v2: i32):
            v9 = icmp_imm eq v2, 55
            v10 = iconst.i32 0
            v11 = iconst.i32 1
            brif v9, block3(v10), block3(v11)  ; v10 = 0, v11 = 1

        block3(v3: i32):
            return v3
        }
         */
        let mut generator = Generator::new_object_file("main");

        // define function
        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let mut func_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func, &mut generator.function_builder_context);

            // start
            //   |
            //   |          jump(0, 10)
            //   v
            // block_loop  (sum, n)
            //   |          sum' = sum + n
            //   |          n'   = n - 1
            //   |          recur block0 if n != 0
            //   v
            // block_check (sum)
            //   |
            //   v
            // block_exit

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_loop = func_builder.create_block();
            func_builder.append_block_param(block_loop, types::I32);
            func_builder.append_block_param(block_loop, types::I32);

            let block_check = func_builder.create_block();
            func_builder.append_block_param(block_check, types::I32);

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);
            let value_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let value_imm_10 = func_builder.ins().iconst(types::I32, 10);
            func_builder
                .ins()
                .jump(block_loop, &[value_imm_0, value_imm_10]);

            // build block_check0
            func_builder.switch_to_block(block_loop);

            let value_params = func_builder.block_params(block_loop).to_vec();
            let value_sum = value_params[0];
            let value_n = value_params[1];
            let value_sum_prime = func_builder.ins().iadd(value_sum, value_n);
            let value_n_prime = func_builder.ins().iadd_imm(value_n, -1);

            let cmp_result = func_builder.ins().icmp_imm(IntCC::Equal, value_n_prime, 0);

            func_builder.ins().brif(
                cmp_result,
                block_check,
                &[value_sum_prime],
                block_loop,
                &[value_sum_prime, value_n_prime],
            );

            // build block_check
            func_builder.switch_to_block(block_check);
            let value_param_sum = func_builder.block_params(block_check)[0];
            let cmp_result = func_builder
                .ins()
                .icmp_imm(IntCC::Equal, value_param_sum, 55);

            let value_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let value_imm_1 = func_builder.ins().iconst(types::I32, 1);

            func_builder.ins().brif(
                cmp_result,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_1],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code(
            &module_binary,
            "anna_unit_test_utils_control_flow",
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_utils_import_function() {
        /*
        function u0:1() -> i32 system_v {
            sig0 = (i32, i32) -> i32 system_v
            fn0 = u0:0 sig0

        block0:
            v1 = iconst.i32 11
            v2 = iconst.i32 13
            v3 = call fn0(v1, v2)  ; v1 = 11, v2 = 13
            v4 = icmp_imm eq v3, 24
            v5 = iconst.i32 0
            v6 = iconst.i32 1
            brif v4, block1(v5), block1(v6)  ; v5 = 0, v6 = 1

        block1(v0: i32):
            return v0
        }
         */
        let mut generator = Generator::new_object_file("main");

        // import function 'add'
        let mut sig_add = generator.module.make_signature();
        sig_add.params.push(AbiParam::new(types::I32));
        sig_add.params.push(AbiParam::new(types::I32));
        sig_add.returns.push(AbiParam::new(types::I32));

        let func_add_id = generator
            .module
            .declare_function("add", Linkage::Import, &sig_add)
            .unwrap();

        // define function
        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            // there are 2 methods to import a function:
            //
            // - declare_func_in_func: for imporing the function within the same module, but
            //   it can import external function also.
            // - import_function: for importing external function.
            //
            // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/entities/struct.FuncRef.html

            let func_add_ref = generator
                .module
                .declare_func_in_func(func_add_id, &mut func);

            let mut func_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func, &mut generator.function_builder_context);

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);
            let value_imm_11 = func_builder.ins().iconst(types::I32, 11);
            let value_imm_13 = func_builder.ins().iconst(types::I32, 13);
            let inst_call_add = func_builder
                .ins()
                .call(func_add_ref, &[value_imm_11, value_imm_13]);

            let call_result = func_builder.inst_results(inst_call_add)[0];
            let cmp_result = func_builder.ins().icmp_imm(IntCC::Equal, call_result, 24);

            let value_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let value_imm_1 = func_builder.ins().iconst(types::I32, 1);

            func_builder.ins().brif(
                cmp_result,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_1],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code_with_libtest0(
            &module_binary,
            "anna_unit_test_utils_import_function",
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_utils_indirect_function_call() {
        /*
        the target function is obtained dynamically by
        function address.
         */

        /*
        function u0:1() -> i32 system_v {
            sig0 = () -> i64 system_v
            sig1 = (i32, i32) -> i32 system_v
            fn0 = u0:0 sig0

        block0:
            v1 = call fn0()
            v2 = iconst.i32 11
            v3 = iconst.i32 13
            v4 = call_indirect sig1, v1(v2, v3)  ; v2 = 11, v3 = 13
            v5 = icmp_imm eq v4, 24
            v6 = iconst.i32 0
            v7 = iconst.i32 1
            brif v5, block1(v6), block1(v7)  ; v6 = 0, v7 = 1

        block1(v0: i32):
            return v0
        }
         */
        let mut generator = Generator::new_object_file("main");

        let addr_t: Type = generator.module.isa().pointer_type();

        // external function 'add'
        let mut sig_add = generator.module.make_signature();
        sig_add.params.push(AbiParam::new(types::I32));
        sig_add.params.push(AbiParam::new(types::I32));
        sig_add.returns.push(AbiParam::new(types::I32));

        // import function 'get_func_add_address'
        let mut sig_get_func_add_address = generator.module.make_signature();
        sig_get_func_add_address.returns.push(AbiParam::new(addr_t));

        let func_get_func_add_address_id = generator
            .module
            .declare_function("get_func_add_address", Linkage::Import, &sig_get_func_add_address)
            .unwrap();

        // define function
        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let func_get_func_add_address_ref = generator
                .module
                .declare_func_in_func(func_get_func_add_address_id, &mut func);

            let mut func_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func, &mut generator.function_builder_context);

            let sig_add_ref = func_builder.import_signature(sig_add);

            // block_start ---> block_exit

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);

            // get the address of the function 'add'
            let inst_call_get_func_add_address = func_builder.ins().call(func_get_func_add_address_ref, &[]);
            let func_add_addr = func_builder.inst_results(inst_call_get_func_add_address)[0];

            // call function 'add'
            let value_imm_11 = func_builder.ins().iconst(types::I32, 11);
            let value_imm_13 = func_builder.ins().iconst(types::I32, 13);
            let inst_call_add = func_builder.ins().call_indirect(
                sig_add_ref,
                func_add_addr,
                &[value_imm_11, value_imm_13],
            );

            let call_result = func_builder.inst_results(inst_call_add)[0];
            let cmp_result = func_builder.ins().icmp_imm(IntCC::Equal, call_result, 24);

            let value_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let value_imm_1 = func_builder.ins().iconst(types::I32, 1);

            func_builder.ins().brif(
                cmp_result,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_1],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code_with_libtest0(
            &module_binary,
            "anna_unit_test_utils_indirect_function_call",
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_utils_import_data() {
        /*
        function u0:2() -> i32 system_v {
            gv0 = symbol userextname2       ;; normal_var
            sig0 = (i32) system_v
            sig1 = () -> i32 system_v
            fn0 = u0:0 sig0                 ;; inc_normal
            fn1 = u0:1 sig1                 ;; get_normal_var

        block0:
            jump block1

        block1:
            v1 = global_value.i64 gv0
            v2 = load.i32 v1
            v3 = iconst.i32 1
            v4 = icmp_imm eq v2, 0
            brif v4, block2, block7(v3)  ; v3 = 1

        block2:
            v5 = call fn1()
            v6 = iconst.i32 2
            v7 = icmp_imm eq v5, 0
            brif v7, block3, block7(v6)  ; v6 = 2

        block3:
            v8 = iconst.i32 11
            call fn0(v8)  ; v8 = 11
            v9 = load.i32 v1
            v10 = iconst.i32 3
            v11 = icmp_imm eq v9, 11
            brif v11, block4, block7(v10)  ; v10 = 3

        block4:
            v12 = call fn1()
            v13 = iconst.i32 4
            v14 = icmp_imm eq v12, 11
            brif v14, block5, block7(v13)  ; v13 = 4

        block5:
            v15 = iconst.i32 13
            store v15, v1  ; v15 = 13
            v16 = load.i32 v1
            v17 = iconst.i32 5
            v18 = icmp_imm eq v16, 13
            brif v18, block6, block7(v17)  ; v17 = 5

        block6:
            v19 = call fn1()
            v20 = iconst.i32 0
            v21 = iconst.i32 6
            v22 = icmp_imm eq v19, 13
            brif v22, block7(v20), block7(v21)  ; v20 = 0, v21 = 6

        block7(v0: i32):
            return v0
        }
         */
        let mut generator = Generator::new_object_file("main");

        let addr_t: Type = generator.module.isa().pointer_type();

        // import function 'inc_normal'
        let mut sig_inc_normal = generator.module.make_signature();
        sig_inc_normal.params.push(AbiParam::new(types::I32));

        let func_inc_normal_id = generator
            .module
            .declare_function("inc_normal", Linkage::Import, &sig_inc_normal)
            .unwrap();

        // import function 'get_normal_var'
        let mut sig_get_normal_var = generator.module.make_signature();
        sig_get_normal_var.returns.push(AbiParam::new(types::I32));

        let func_get_normal_var_id = generator
            .module
            .declare_function("get_normal_var", Linkage::Import, &sig_get_normal_var)
            .unwrap();

        // import data
        let data_normal_var_id = generator
            .module
            .declare_data("normal_var", Linkage::Import, true, false)
            .unwrap();

        // define function
        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let func_inc_normal_ref = generator
                .module
                .declare_func_in_func(func_inc_normal_id, &mut func);

            let func_get_normal_var_ref = generator
                .module
                .declare_func_in_func(func_get_normal_var_id, &mut func);

            let gv_normal_var = generator
                .module
                .declare_data_in_func(data_normal_var_id, &mut func);

            let mut func_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func, &mut generator.function_builder_context);

            // block_start
            //
            // block_check0     load(normal_var)
            //                  check, assert_eq(0)
            //
            // block_check1     get_normal_var()
            //                  check, assert_eq(0)
            //
            // block_check2     inc_normal(11)
            //                  load(normal_var)
            //                  check, assert_eq(11)
            //
            // block_check3     get_normal_var()
            //                  check, assert_eq(11)
            //
            // block_check4     store(normal_var, 13)
            //                  load(normal_var)
            //                  check, assert_eq(13)
            //
            // block_check5     get_normal_var()
            //                  check, assert_eq(13)
            //
            // block_exit

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_check0 = func_builder.create_block();
            let block_check1 = func_builder.create_block();
            let block_check2 = func_builder.create_block();
            let block_check3 = func_builder.create_block();
            let block_check4 = func_builder.create_block();
            let block_check5 = func_builder.create_block();

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);
            func_builder.ins().jump(block_check0, &[]);

            // bhild block_check0
            func_builder.switch_to_block(block_check0);
            let normal_var_addr = func_builder.ins().global_value(addr_t, gv_normal_var);
            let value_0 = func_builder.ins().load(
                types::I32,
                MemFlags::new(),
                normal_var_addr,
                Offset32::new(0),
            );

            let value_imm_1 = func_builder.ins().iconst(types::I32, 1);
            let cmp_result_0 = func_builder.ins().icmp_imm(IntCC::Equal, value_0, 0);

            func_builder
                .ins()
                .brif(cmp_result_0, block_check1, &[], block_exit, &[value_imm_1]);

            // build block_check1
            func_builder.switch_to_block(block_check1);
            let inst_call_0 = func_builder.ins().call(func_get_normal_var_ref, &[]);
            let value_1 = func_builder.inst_results(inst_call_0)[0];

            let value_imm_2 = func_builder.ins().iconst(types::I32, 2);
            let cmp_result_1 = func_builder.ins().icmp_imm(IntCC::Equal, value_1, 0);

            func_builder
                .ins()
                .brif(cmp_result_1, block_check2, &[], block_exit, &[value_imm_2]);

            // build block_check2
            func_builder.switch_to_block(block_check2);
            let value_imm_11 = func_builder.ins().iconst(types::I32, 11);
            func_builder
                .ins()
                .call(func_inc_normal_ref, &[value_imm_11]);

            let value_2 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), normal_var_addr, 0);
            let value_imm_3 = func_builder.ins().iconst(types::I32, 3);
            let cmp_result_2 = func_builder.ins().icmp_imm(IntCC::Equal, value_2, 11);

            func_builder
                .ins()
                .brif(cmp_result_2, block_check3, &[], block_exit, &[value_imm_3]);

            // build block_check3
            func_builder.switch_to_block(block_check3);
            let inst_call_1 = func_builder.ins().call(func_get_normal_var_ref, &[]);
            let value_3 = func_builder.inst_results(inst_call_1)[0];

            let value_imm_4 = func_builder.ins().iconst(types::I32, 4);
            let cmp_result_3 = func_builder.ins().icmp_imm(IntCC::Equal, value_3, 11);

            func_builder
                .ins()
                .brif(cmp_result_3, block_check4, &[], block_exit, &[value_imm_4]);

            // build block_check4
            func_builder.switch_to_block(block_check4);
            let value_imm_13 = func_builder.ins().iconst(types::I32, 13);
            func_builder
                .ins()
                .store(MemFlags::new(), value_imm_13, normal_var_addr, 0);

            let value_4 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), normal_var_addr, 0);
            let value_imm_5 = func_builder.ins().iconst(types::I32, 5);
            let cmp_result_4 = func_builder.ins().icmp_imm(IntCC::Equal, value_4, 13);

            func_builder
                .ins()
                .brif(cmp_result_4, block_check5, &[], block_exit, &[value_imm_5]);

            // build block_check5
            func_builder.switch_to_block(block_check5);
            let inst_call_2 = func_builder.ins().call(func_get_normal_var_ref, &[]);
            let value_5 = func_builder.inst_results(inst_call_2)[0];

            let value_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let value_imm_6 = func_builder.ins().iconst(types::I32, 6);
            let cmp_result_5 = func_builder.ins().icmp_imm(IntCC::Equal, value_5, 13);

            func_builder.ins().brif(
                cmp_result_5,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_6],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code_with_libtest0(
            &module_binary,
            "anna_unit_test_utils_import_data",
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_utils_import_tls_data() {
        /*
        function u0:2() -> i32 system_v {
            gv0 = symbol tls userextname2       ;; tls_var
            sig0 = (i32) system_v
            sig1 = () -> i32 system_v
            fn0 = u0:0 sig0                     ;; inc_tls
            fn1 = u0:1 sig1                     ;; get_tls_var

        block0:
            jump block1

        block1:
            v1 = tls_value.i64 gv0
            v2 = load.i32 v1
            v3 = iconst.i32 1
            v4 = icmp_imm eq v2, 0
            brif v4, block2, block7(v3)  ; v3 = 1

        block2:
            v5 = call fn1()
            v6 = iconst.i32 2
            v7 = icmp_imm eq v5, 0
            brif v7, block3, block7(v6)  ; v6 = 2

        block3:
            v8 = iconst.i32 11
            call fn0(v8)  ; v8 = 11
            v9 = load.i32 v1
            v10 = iconst.i32 3
            v11 = icmp_imm eq v9, 0
            brif v11, block4, block7(v10)  ; v10 = 3

        block4:
            v12 = call fn1()
            v13 = iconst.i32 4
            v14 = icmp_imm eq v12, 0
            brif v14, block5, block7(v13)  ; v13 = 4

        block5:
            v15 = iconst.i32 13
            store v15, v1  ; v15 = 13
            v16 = load.i32 v1
            v17 = iconst.i32 5
            v18 = icmp_imm eq v16, 13
            brif v18, block6, block7(v17)  ; v17 = 5

        block6:
            v19 = call fn1()
            v20 = iconst.i32 0
            v21 = iconst.i32 6
            v22 = icmp_imm eq v19, 13
            brif v22, block7(v20), block7(v21)  ; v20 = 0, v21 = 6

        block7(v0: i32):
            return v0
        }
         */

        let mut generator = Generator::new_object_file("main");

        let addr_t: Type = generator.module.isa().pointer_type();

        // import function 'inc_tls'
        let mut sig_inc_tls = generator.module.make_signature();
        sig_inc_tls.params.push(AbiParam::new(types::I32));

        let func_inc_tls_id = generator
            .module
            .declare_function("inc_tls", Linkage::Import, &sig_inc_tls)
            .unwrap();

        // import function 'get_tls_var'
        let mut sig_get_tls_var = generator.module.make_signature();
        sig_get_tls_var.returns.push(AbiParam::new(types::I32));

        let func_get_tls_var_id = generator
            .module
            .declare_function("get_tls_var", Linkage::Import, &sig_get_tls_var)
            .unwrap();

        // import data
        let data_tls_var_id = generator
            .module
            .declare_data("tls_var", Linkage::Import, true, true)
            .unwrap();

        // define function
        let mut sig_main = generator.module.make_signature();
        sig_main.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &sig_main)
            .unwrap();

        {
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                sig_main,
            );

            let func_inc_tls_ref = generator
                .module
                .declare_func_in_func(func_inc_tls_id, &mut func);

            let func_get_tls_var_ref = generator
                .module
                .declare_func_in_func(func_get_tls_var_id, &mut func);

            let gv_tls_var = generator
                .module
                .declare_data_in_func(data_tls_var_id, &mut func);

            let mut func_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func, &mut generator.function_builder_context);

            // block_start
            //
            // block_check0     load(tls_var)
            //                  check, assert_eq(0)
            //
            // block_check1     get_tls_var()
            //                  check, assert_eq(0)
            //
            // block_check2     inc_tls(11)
            //                  load(tls_var)
            //                  check, assert_eq(0)
            //
            // block_check3     get_tls_var()
            //                  check, assert_eq(0)
            //
            // block_check4     store(tls_var, 13)
            //                  load(tls_var)
            //                  check, assert_eq(13)
            //
            // block_check5     get_tls_var()
            //                  check, assert_eq(13)
            //
            // block_exit

            let block_start = func_builder.create_block();
            func_builder.append_block_params_for_function_params(block_start);

            let block_check0 = func_builder.create_block();
            let block_check1 = func_builder.create_block();
            let block_check2 = func_builder.create_block();
            let block_check3 = func_builder.create_block();
            let block_check4 = func_builder.create_block();
            let block_check5 = func_builder.create_block();

            let block_exit = func_builder.create_block();
            func_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            func_builder.switch_to_block(block_start);
            func_builder.ins().jump(block_check0, &[]);

            // bhild block_check0
            func_builder.switch_to_block(block_check0);

            // note:
            // - tls_value()
            // - global_value()
            let tls_var_addr = func_builder.ins().tls_value(addr_t, gv_tls_var);
            let value_0 = func_builder.ins().load(
                types::I32,
                MemFlags::new(),
                tls_var_addr,
                Offset32::new(0),
            );

            let value_imm_1 = func_builder.ins().iconst(types::I32, 1);
            let cmp_result_0 = func_builder.ins().icmp_imm(IntCC::Equal, value_0, 0);

            func_builder
                .ins()
                .brif(cmp_result_0, block_check1, &[], block_exit, &[value_imm_1]);

            // build block_check1
            func_builder.switch_to_block(block_check1);
            let inst_call_0 = func_builder.ins().call(func_get_tls_var_ref, &[]);
            let value_1 = func_builder.inst_results(inst_call_0)[0];

            let value_imm_2 = func_builder.ins().iconst(types::I32, 2);
            let cmp_result_1 = func_builder.ins().icmp_imm(IntCC::Equal, value_1, 0);

            func_builder
                .ins()
                .brif(cmp_result_1, block_check2, &[], block_exit, &[value_imm_2]);

            // build block_check2
            func_builder.switch_to_block(block_check2);
            let value_imm_11 = func_builder.ins().iconst(types::I32, 11);
            func_builder.ins().call(func_inc_tls_ref, &[value_imm_11]);

            let value_2 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), tls_var_addr, 0);
            let value_imm_3 = func_builder.ins().iconst(types::I32, 3);
            let cmp_result_2 = func_builder.ins().icmp_imm(IntCC::Equal, value_2, 0);

            func_builder
                .ins()
                .brif(cmp_result_2, block_check3, &[], block_exit, &[value_imm_3]);

            // build block_check3
            func_builder.switch_to_block(block_check3);
            let inst_call_1 = func_builder.ins().call(func_get_tls_var_ref, &[]);
            let value_3 = func_builder.inst_results(inst_call_1)[0];

            let value_imm_4 = func_builder.ins().iconst(types::I32, 4);
            let cmp_result_3 = func_builder.ins().icmp_imm(IntCC::Equal, value_3, 0);

            func_builder
                .ins()
                .brif(cmp_result_3, block_check4, &[], block_exit, &[value_imm_4]);

            // build block_check4
            func_builder.switch_to_block(block_check4);
            let value_imm_13 = func_builder.ins().iconst(types::I32, 13);
            func_builder
                .ins()
                .store(MemFlags::new(), value_imm_13, tls_var_addr, 0);

            let value_4 = func_builder
                .ins()
                .load(types::I32, MemFlags::new(), tls_var_addr, 0);
            let value_imm_5 = func_builder.ins().iconst(types::I32, 5);
            let cmp_result_4 = func_builder.ins().icmp_imm(IntCC::Equal, value_4, 13);

            func_builder
                .ins()
                .brif(cmp_result_4, block_check5, &[], block_exit, &[value_imm_5]);

            // build block_check5
            func_builder.switch_to_block(block_check5);
            let inst_call_2 = func_builder.ins().call(func_get_tls_var_ref, &[]);
            let value_5 = func_builder.inst_results(inst_call_2)[0];

            let value_imm_0 = func_builder.ins().iconst(types::I32, 0);
            let value_imm_6 = func_builder.ins().iconst(types::I32, 6);
            let cmp_result_5 = func_builder.ins().icmp_imm(IntCC::Equal, value_5, 13);

            func_builder.ins().brif(
                cmp_result_5,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_6],
            );

            // build block_exit
            func_builder.switch_to_block(block_exit);

            let exit_code_value = func_builder.block_params(block_exit)[0];
            func_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            func_builder.seal_all_blocks();
            func_builder.finalize();

            println!("{}", func.display());

            generator.context.func = func;

            generator
                .module
                .define_function(func_main_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // finish the module
        let object_procduct = generator.module.finish();
        let module_binary = object_procduct.emit().unwrap();
        let exit_code_opt = run_executable_binary_and_get_exit_code_with_libtest0(
            &module_binary,
            "anna_unit_test_utils_import_tls_data",
        );

        assert_eq!(exit_code_opt, Some(0));
    }
}
