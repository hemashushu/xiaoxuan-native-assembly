// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    process::{Command, ExitStatus},
};

fn get_temp_file_fullpath(filename: &str) -> String {
    let mut dir = std::env::temp_dir();
    dir.push(filename);
    dir.to_str().unwrap().to_owned()
}

fn link_single_object_file_as_executable_file(
    object_file_path: &str,
    external_library_folder_path: Option<&str>,
    external_library_link_name: Option<&str>,
    output_file_path: &str,
) -> std::io::Result<ExitStatus> {
    // linking examples
    // ----------------
    //
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
    // ref:
    // check the result of command `$ gcc -v -o anna.elf anna.o`

    // Mini FAQ about the misc libc/gcc crt files
    // ------------------------------------------
    //
    // From: https://dev.gentoo.org/~vapier/crt.txt
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
    //
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
    // Note that the file 'Scrt1.o' is owned by package 'glibc', check:
    // `$ pacman -Qo Scrt1.o`
    // `$ pacman -Ql glibc | grep crt`

    // shared library names
    // --------------------
    //
    // Shared libraries essentially have three names:
    //
    // - soname (logical name): The soname follows this naming scheme:
    //   `lib<library name>.so.<version number>`.
    //   e.g. `libcurl.so.4`,
    // - real name: That has a base filename consisting of the soname plus
    //   `.<minor number>.<release number>` (although the .<release number> is optional.
    //   e.g. `libcurl.so.4.8.0`
    // - link name: The link name is the soname without any version numbering.
    //   e.g. `libcurl.so`. The 'lib' and '.so' can be omitted when pass the link name to `GCC` and `ld`.
    //
    // P.S., generate a shared library with soname specified:
    // `gcc -Wall -g -fpic -shared -Wl,-soname,libtest0.so.1 -o libtest0.so.1.0.0 libtest0.c``

    let mut args = vec![];

    args.push("--dynamic-linker");
    args.push("/lib64/ld-linux-x86-64.so.2");
    args.push("-pie");
    args.push("-o");
    args.push(output_file_path);
    args.push("/usr/lib/Scrt1.o");
    args.push("/usr/lib/crti.o");
    args.push("-L/lib/");
    args.push("-L/usr/lib");

    if let Some(lib_path_str) = external_library_folder_path {
        args.push("-L");
        args.push(lib_path_str);
    }

    args.push(object_file_path);

    if let Some(lib_linkname_str) = external_library_link_name {
        args.push("-l");
        args.push(lib_linkname_str);
    }

    args.push("-lc");
    args.push("/usr/lib/crtn.o");

    // Command::new("/usr/bin/ld").args(args).status()
    Command::new("ld").args(args).status()
}

fn static_link_single_object_file_as_executable_file_with_musl(
    object_file_path: &str,
    usr_lib_musl_lib_path: Option<&str>,
    external_library_object_file_path: Option<&str>,
    output_file_path: &str,
) -> std::io::Result<ExitStatus> {
    // linking with MUSL
    // -----------------
    //
    // ```sh
    // ld \
    //     -dynamic-linker /lib/ld-musl-x86_64.so.1 \
    //     -nostdlib \
    //     -pie \
    //     -o test_libc.elf \
    //     /usr/lib/musl/lib/Scrt1.o \
    //     /usr/lib/musl/lib/crti.o \
    //     -L/usr/lib/musl/lib \
    //     test_libc.o \
    //     -lc \
    //     /usr/lib/musl/lib/crtn.o
    // ```
    //
    // and check the dynamic link list:
    //
    // `$ /lib/ld-musl-x86_64.so.1 --list test_libc.elf`
    //
    // replace the "-pie" above with "-static" to generate static linking
    // executable file.
    //
    // ref:
    // check the result of command `$ musl-gcc -v -o test_libc.elf test_libc.o`

    let musl_lib = if let Some(p) = usr_lib_musl_lib_path {
        p
    } else {
        "/usr/lib/musl/lib"
    };

    let mut args = vec![];

    // args.push("--dynamic-linker");
    // args.push("/lib/ld-musl-x86_64.so.1");
    // args.push("-pie");
    args.push("-nostdlib".to_owned());
    args.push("-static".to_owned());
    args.push("-o".to_owned());
    args.push(output_file_path.to_owned());
    args.push(format!("{musl_lib}/Scrt1.o"));
    args.push(format!("{musl_lib}/crti.o"));
    args.push(format!("-L{musl_lib}"));
    args.push(object_file_path.to_owned());

    if let Some(lib_object_file_path) = external_library_object_file_path {
        args.push(lib_object_file_path.to_owned());
    }

    args.push("-lc".to_owned());
    args.push(format!("{musl_lib}/crtn.o"));

    Command::new("ld").args(args).status()
}

fn delete_file(filepath: &str) {
    std::fs::remove_file(filepath).unwrap();
}

fn get_tests_lib_folder_path() -> String {
    let mut pwd = std::env::current_dir().unwrap();

    // the name of package that is written within the file 'Cargo.toml'
    // `let pkg_name = env!("CARGO_PKG_NAME");`
    //
    // the name of package which is convert '-' to '_'.
    // `let crate_name = env!("CARGO_CRATE_NAME");`
    //
    // ref:
    // https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates
    let crate_dir_name = env!("CARGO_MANIFEST_DIR");
    if !pwd.ends_with(crate_dir_name) {
        // in the VSCode editor's `Debug` processing, the `current_dir()` returns
        // the project's root folder.
        // while in both command `$ cargo test` and VSCode editor's `Run Test` processing,
        // the `current_dir()` returns the current crate path.
        // the following canonicalize the test resources path.
        pwd.push("crates");
        pwd.push(crate_dir_name);
    }

    pwd.push("tests");
    pwd.push("lib");
    pwd.to_str().unwrap().to_string()
}

fn get_tests_lib_file_path(filename: &str) -> String {
    let mut folder = PathBuf::from(get_tests_lib_folder_path());
    folder.push(filename);
    folder.to_str().unwrap().to_string()
}

fn run_executable_binary_and_get_exit_code(
    binary: &[u8],
    program_name: &str,
    static_link: bool,
) -> Option<i32> {
    // write object file `*.o`
    let object_file_path = get_temp_file_fullpath(&format!("{}.o", program_name));
    let mut file = File::create(&object_file_path).unwrap();
    file.write_all(&binary).unwrap();

    // link file as `*.elf`
    let exec_file_path = get_temp_file_fullpath(&format!("{}.elf", program_name));

    if static_link {
        static_link_single_object_file_as_executable_file_with_musl(
            &object_file_path,
            None,
            None,
            &exec_file_path,
        )
        .unwrap();
    } else {
        link_single_object_file_as_executable_file(&object_file_path, None, None, &exec_file_path)
            .unwrap();
    }

    // Run the executable file and get the exit code, e.g.
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
    binary: &[u8],
    program_name: &str,
    static_link: bool,
) -> Option<i32> {
    // write object file `*.o`
    let object_file_path = get_temp_file_fullpath(&format!("{}.o", program_name));
    let mut file = File::create(&object_file_path).unwrap();
    file.write_all(&binary).unwrap();

    // link file as `*.elf`
    let exec_file_path = get_temp_file_fullpath(&format!("{}.elf", program_name));

    let exit_code_opt = if static_link {
        let user_lib_object_filename = "libtest0.o";
        let user_lib_object_filepath = get_tests_lib_file_path(&user_lib_object_filename);

        static_link_single_object_file_as_executable_file_with_musl(
            &object_file_path,
            None,
            Some(&user_lib_object_filepath),
            &exec_file_path,
        )
        .unwrap();

        Command::new(&exec_file_path).status().unwrap().code()
    } else {
        let user_lib_folder_path = get_tests_lib_folder_path();
        let user_lib_linkname = "test0";
        link_single_object_file_as_executable_file(
            &object_file_path,
            Some(&user_lib_folder_path),
            Some(user_lib_linkname),
            &exec_file_path,
        )
        .unwrap();

        // run executable file and get exit code
        Command::new(&exec_file_path)
            .env("LD_LIBRARY_PATH", &user_lib_folder_path)
            .status()
            .unwrap()
            .code()
    };

    // clean up
    delete_file(&object_file_path);
    delete_file(&exec_file_path);

    exit_code_opt
}

#[cfg(test)]
mod tests {
    use cranelift_codegen::ir::{
        condcodes::IntCC, immediates::Offset32, types, AbiParam, ExtFuncData, ExternalName,
        Function, InstBuilder, MemFlags, SigRef, StackSlotData, StackSlotKind, Type,
        UserExternalNameRef, UserFuncName,
    };
    use cranelift_frontend::FunctionBuilder;
    use cranelift_module::{Linkage, Module};
    use cranelift_object::ObjectModule;

    use crate::{
        code_generator::Generator,
        utils::{
            run_executable_binary_and_get_exit_code,
            run_executable_binary_and_get_exit_code_with_libtest0,
        },
    };

    #[test]
    fn test_code_generator_object() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        // build function "inc"
        //
        // ```rust
        // fn inc (a:i32) -> i32 {
        //    a+11
        // }
        // ```

        let mut func_inc_sig = generator.module.make_signature();
        func_inc_sig.params.push(AbiParam::new(types::I32));
        func_inc_sig.returns.push(AbiParam::new(types::I32));

        let func_inc_id = generator
            .module
            .declare_function("inc", Linkage::Local, &func_inc_sig)
            .unwrap();

        {
            let mut func_inc = Function::with_name_signature(
                UserFuncName::user(0, func_inc_id.as_u32()),
                func_inc_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_inc, &mut generator.function_builder_context);

            let block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block);

            function_builder.switch_to_block(block);

            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.block_params(block)[0];
            let value_2 = function_builder.ins().iadd(value_0, value_1);
            function_builder.ins().return_(&[value_2]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // to display the text of IR
            // println!("{}", func_inc.display());

            // generate func_inc body's (machine/native) code

            generator.context.func = func_inc;

            generator
                .module
                .define_function(func_inc_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
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
                .declare_func_in_func(func_inc_id, function_builder.func);

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

            // to display the text of IR
            // println!("{}", func_main.display());

            // generate func_main body's (machine/native) code

            generator.context.func = func_main;

            generator
                .module
                .define_function(func_main_delcare, &mut generator.context)
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
            "test_code_generator_object",
            false,
        );

        assert_eq!(exit_code_opt, Some(24));
    }

    #[test]
    fn test_code_generator_control_flow_branch() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        // build function "swap"
        //
        // ```rust
        // fn swap (a:i32, b:i32) -> (i32, i32) {
        //    (b, a)
        // }
        // ```

        let mut func_swap_sig = generator.module.make_signature();
        func_swap_sig.params.push(AbiParam::new(types::I32));
        func_swap_sig.params.push(AbiParam::new(types::I32));
        func_swap_sig.returns.push(AbiParam::new(types::I32));
        func_swap_sig.returns.push(AbiParam::new(types::I32));

        let func_swap_id = generator
            .module
            .declare_function("swap", Linkage::Local, &func_swap_sig)
            .unwrap();

        {
            let mut func_swap = Function::with_name_signature(
                UserFuncName::user(0, func_swap_id.as_u32()),
                func_swap_sig,
            );

            let mut function_builder: FunctionBuilder = FunctionBuilder::new(
                // &mut generator.context.func,
                &mut func_swap,
                &mut generator.function_builder_context,
            );
            let block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block);
            function_builder.switch_to_block(block);

            let value_a = function_builder.block_params(block)[0];
            let value_b = function_builder.block_params(block)[1];

            // return (b, a)
            function_builder.ins().return_(&[value_b, value_a]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // to display the text of IR
            // println!("{}", func_inc.display());

            // generate the function code

            generator.context.func = func_swap;

            generator
                .module
                .define_function(func_swap_id, &mut generator.context)
                .unwrap();

            generator.module.clear_context(&mut generator.context);
        }

        // build function "main"
        //
        // ```rust
        // fn main () -> i32 {
        //    let (a, b) = swap(11,13)
        //    if a == 13 {
        //       if b == 11 {
        //          0
        //       }else {
        //          2
        //       }
        //    }
        //    else {
        //       1
        //    }
        // }
        // ```

        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let func_swap_ref = generator
                .module
                .declare_func_in_func(func_swap_id, &mut func_main);

            let mut function_builder: FunctionBuilder = FunctionBuilder::new(
                // &mut generator.context.func,
                &mut func_main,
                &mut generator.function_builder_context,
            );

            // ()                                 (i32)
            // start ---> check0 ---> check1 ---> exit
            //                    |           ^
            //                    \-----------/

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_check0 = function_builder.create_block();
            let block_check1 = function_builder.create_block();

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);

            // call swap(11, 13)
            // results == (13, 11)
            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.ins().iconst(types::I32, 13);

            let call0 = function_builder
                .ins()
                .call(func_swap_ref, &[value_0, value_1]);
            let call0_results = function_builder.inst_results(call0).to_vec();
            function_builder.ins().jump(block_check0, &[]);

            // build block_check0
            // assert results[0] == 13
            function_builder.switch_to_block(block_check0);

            let check_result_0 =
                function_builder
                    .ins()
                    .icmp_imm(IntCC::Equal, call0_results[0], 13);
            let exit_code_imm_1 = function_builder.ins().iconst(types::I32, 1);

            function_builder.ins().brif(
                check_result_0,
                block_check1,
                &[],
                block_exit,
                &[exit_code_imm_1],
            );

            // build block_check1
            // assert results[1] == 11
            function_builder.switch_to_block(block_check1);

            let check_result_1 =
                function_builder
                    .ins()
                    .icmp_imm(IntCC::Equal, call0_results[1], 11);
            let exit_code_imm_2 = function_builder.ins().iconst(types::I32, 2);
            let exit_code_imm_0 = function_builder.ins().iconst(types::I32, 0);

            function_builder.ins().brif(
                check_result_1,
                block_exit,
                &[exit_code_imm_0],
                block_exit,
                &[exit_code_imm_2],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            // generate the function code

            generator.context.func = func_main;

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
            "test_code_generator_control_flow_branch",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_code_generator_data() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        let pointer_t: Type = generator.module.isa().pointer_type();

        // the process reading a data (which is inside .data/.ro_data/.bss):
        // 1. let gv = construct a GlobalValue object, e.g. module.declare_data_in_func(...)
        // 2. let target_address = ins().symbol_value(gv)
        // 3. let value = ins().load(target_address)
        //
        // ins().global_value(GV) -> addr: Compute the value of global GV
        // ins().symbol_value(GV) -> addr: Compute the value of global GV, which is a symbolic value.
        // ins().tls_value(GV) -> addr: Compute the value of global GV, which is a TLS (thread local storage) value.
        //
        // note: it seems both global_value() and symbol_value() work.

        // define a read-only data
        let data_number0_content = 11u32.to_le_bytes().to_vec();
        let data_number0_id = generator
            .define_initialized_data("number0", data_number0_content, 4, false, false, false)
            .unwrap();

        // define a read-write data
        let data_number1_content = 13u32.to_le_bytes().to_vec();
        let data_number1_id = generator
            .define_initialized_data("number1", data_number1_content, 4, false, true, false)
            .unwrap();

        // define function "main"
        //
        // ```rust
        // fn main() -> i32 {
        //    if load(number0_addr) == 11 {
        //        if load(number1_addr) == 13 {
        //           store(number1_addr, 17)
        //           if load(number1_addr) == 17 {
        //              0
        //           }else {
        //              3
        //           }
        //        }
        //        else {
        //           2
        //        }
        //     }else {
        //        1
        //     }
        // }
        // ```

        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        // the function 'main' should be 'export', so the linker can find it.
        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let gv_data_number0 = generator
                .module
                .declare_data_in_func(data_number0_id, &mut func_main);
            let gv_data_number1 = generator
                .module
                .declare_data_in_func(data_number1_id, &mut func_main);

            let mut function_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            // blocks
            // ------
            //                                    update and
            //            check ro    check rw    check rw
            // start ---> check0 ---> check1 ---> check2  ---> exit
            //                    |           |            ^
            //                    |           \------------|
            //                    \------------------------/

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_check0 = function_builder.create_block();
            let block_check1 = function_builder.create_block();
            let block_check2 = function_builder.create_block();

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);
            function_builder.ins().jump(block_check0, &[]);

            // build block_check0
            // assert data0 == 11
            function_builder.switch_to_block(block_check0);
            let data_number0_addr = function_builder
                .ins()
                .symbol_value(pointer_t, gv_data_number0);
            let value_data_number0 =
                function_builder
                    .ins()
                    .load(types::I32, MemFlags::new(), data_number0_addr, 0);

            let check_result_0 =
                function_builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value_data_number0, 11);
            let exit_code_imm_1 = function_builder.ins().iconst(types::I32, 1);

            function_builder.ins().brif(
                check_result_0,
                block_check1,
                &[],
                block_exit,
                &[exit_code_imm_1],
            );

            // build block_check1
            // assert data1 == 13
            function_builder.switch_to_block(block_check1);
            let data_number1_addr = function_builder
                .ins()
                .symbol_value(pointer_t, gv_data_number1);
            let value_data_number1 =
                function_builder
                    .ins()
                    .load(types::I32, MemFlags::new(), data_number1_addr, 0);

            let check_result_1 =
                function_builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value_data_number1, 13);
            let exit_code_imm_2 = function_builder.ins().iconst(types::I32, 2);

            function_builder.ins().brif(
                check_result_1,
                block_check2,
                &[],
                block_exit,
                &[exit_code_imm_2],
            );

            // build block_check2
            // write 17 to data1, and read and assert it is 17
            function_builder.switch_to_block(block_check2);
            let value_imm_17 = function_builder.ins().iconst(types::I32, 17);
            function_builder
                .ins()
                .store(MemFlags::new(), value_imm_17, data_number1_addr, 0);

            let value_data_number1_updated =
                function_builder
                    .ins()
                    .load(types::I32, MemFlags::new(), data_number1_addr, 0);

            let check_result_2 =
                function_builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value_data_number1_updated, 17);
            let exit_code_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let exit_code_imm_3 = function_builder.ins().iconst(types::I32, 3);

            function_builder.ins().brif(
                check_result_2,
                block_exit,
                &[exit_code_imm_0],
                block_exit,
                &[exit_code_imm_3],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            generator.context.func = func_main;

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
            "test_code_generator_data",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_code_generator_local_variable() {
        // 'local variable' is implemented by 'stack_slot' in Cranelift

        let mut generator = Generator::<ObjectModule>::new("main", None);

        let pointer_t: Type = generator.module.isa().pointer_type();

        // define function main
        //
        // fn main() -> i32 {
        //     let mut slot = [0_u8; 4];
        //     stack_store(slot, 11);
        //     if stack_load(slot) == 11 {
        //         let addr = &slot;
        //         if memory_load(addr) == 11 {
        //             memory_store(addr, 17);
        //             if stack_load(slot) == 17 {
        //                 0
        //             }else {
        //                 3
        //             }
        //         }else {
        //             2
        //         }
        //     }else {
        //         1
        //     }
        // }
        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let ss0 = func_main.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                4,
                4,
            ));

            let mut function_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            // blocks
            // ------
            //                         load ss0    store ss0
            //            stack load   by          by mem.store and
            //            ss0          mem.load    stack load ss0
            // start ---> check0 ---> check1 ---> check2 ---> exit
            //                    |           |           ^
            //                    |           \-----------|
            //                    \-----------------------/

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_check0 = function_builder.create_block();
            let block_check1 = function_builder.create_block();
            let block_check2 = function_builder.create_block();

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);
            function_builder.ins().jump(block_check0, &[]);

            // build block_check0
            function_builder.switch_to_block(block_check0);

            let value_imm_11 = function_builder.ins().iconst(types::I32, 11);
            function_builder
                .ins()
                .stack_store(value_imm_11, ss0, Offset32::new(0));

            let value_0 = function_builder.ins().stack_load(types::I32, ss0, 0);

            let check_result_0 = function_builder.ins().icmp_imm(IntCC::Equal, value_0, 11);
            let exit_code_imm_1 = function_builder.ins().iconst(types::I32, 1);

            function_builder.ins().brif(
                check_result_0,
                block_check1,
                &[],
                block_exit,
                &[exit_code_imm_1],
            );

            // build block_check1
            function_builder.switch_to_block(block_check1);
            let local_var_addr = function_builder.ins().stack_addr(pointer_t, ss0, 0);
            let value_1 =
                function_builder
                    .ins()
                    .load(types::I32, MemFlags::new(), local_var_addr, 0);

            let check_result_1 = function_builder.ins().icmp_imm(IntCC::Equal, value_1, 11);
            let exit_code_imm_2 = function_builder.ins().iconst(types::I32, 2);

            function_builder.ins().brif(
                check_result_1,
                block_check2,
                &[],
                block_exit,
                &[exit_code_imm_2],
            );

            // build block_check2
            function_builder.switch_to_block(block_check2);
            let value_imm_17 = function_builder.ins().iconst(types::I32, 17);
            function_builder
                .ins()
                .store(MemFlags::new(), value_imm_17, local_var_addr, 0);

            let value_2 = function_builder.ins().stack_load(types::I32, ss0, 0);

            let check_result_2 = function_builder.ins().icmp_imm(IntCC::Equal, value_2, 17);
            let exit_code_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let exit_code_imm_3 = function_builder.ins().iconst(types::I32, 3);

            function_builder.ins().brif(
                check_result_2,
                block_exit,
                &[exit_code_imm_0],
                block_exit,
                &[exit_code_imm_3],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            generator.context.func = func_main;

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
            "test_code_generator_local_variable",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_code_generator_control_flow_loop() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        // define function "main"
        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            // start
            //   |
            //   |         jump with (0, 10)
            //   v
            // block_loop  (sum, n)                             <----\
            //   |          sum' = sum + n                           |
            //   |          n'   = n - 1                             |
            //   |          if n != 0                                |
            //   |             recur to block_loop with (sum', n') --/
            //   |          else
            //   |             jump to block_check with (sum')
            //   v
            // block_check (sum)
            //   |
            //   |          if sum == 55
            //   |             jump to block_exit with 0
            //   |          else
            //   |             jump to block_exit with 1
            //   v
            // block_exit

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_loop = function_builder.create_block();
            function_builder.append_block_param(block_loop, types::I32);
            function_builder.append_block_param(block_loop, types::I32);

            let block_check = function_builder.create_block();
            function_builder.append_block_param(block_check, types::I32);

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);
            let value_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let value_imm_10 = function_builder.ins().iconst(types::I32, 10);
            function_builder
                .ins()
                .jump(block_loop, &[value_imm_0, value_imm_10]);

            // build block_loop
            function_builder.switch_to_block(block_loop);

            let value_params = function_builder.block_params(block_loop).to_vec();
            let value_sum = value_params[0];
            let value_n = value_params[1];
            let value_sum_prime = function_builder.ins().iadd(value_sum, value_n);
            let value_n_prime = function_builder.ins().iadd_imm(value_n, -1);

            let cmp_result = function_builder
                .ins()
                .icmp_imm(IntCC::Equal, value_n_prime, 0);

            function_builder.ins().brif(
                cmp_result,
                block_check,
                &[value_sum_prime],
                block_loop,
                &[value_sum_prime, value_n_prime],
            );

            // build block_check
            function_builder.switch_to_block(block_check);
            let value_param_sum = function_builder.block_params(block_check)[0];
            let cmp_result = function_builder
                .ins()
                .icmp_imm(IntCC::Equal, value_param_sum, 55);

            let value_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let value_imm_1 = function_builder.ins().iconst(types::I32, 1);

            function_builder.ins().brif(
                cmp_result,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_1],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            generator.context.func = func_main;

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
            "test_code_generator_control_flow_loop",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_code_generator_import_function() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        // import function 'add'
        // `fn add(i32, i32) -> i32`
        let mut func_add_sig = generator.module.make_signature();
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.returns.push(AbiParam::new(types::I32));

        let func_add_id = generator
            .module
            .declare_function("add", Linkage::Import, &func_add_sig)
            .unwrap();

        // define function "main"
        //
        // ```rust
        // fn main() -> i32 {
        //     add(11, 13)
        // }
        // ```

        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            // FuncRefs are used for direct function calls and by func_addr for use in indirect function calls.
            //
            // FuncRefs can be created with
            //
            // - FunctionBuilder::import_function for external functions
            // - Module::declare_func_in_func for functions declared elsewhere in the same native Module
            //
            // While the order is stable, it is arbitrary.
            //
            // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/entities/struct.FuncRef.html

            let func_add_ref = generator
                .module
                .declare_func_in_func(func_add_id, &mut func_main);

            let mut function_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);
            let value_imm_11 = function_builder.ins().iconst(types::I32, 11);
            let value_imm_13 = function_builder.ins().iconst(types::I32, 13);
            let inst_call_add = function_builder
                .ins()
                .call(func_add_ref, &[value_imm_11, value_imm_13]);

            let call_result = function_builder.inst_results(inst_call_add)[0];
            let cmp_result = function_builder
                .ins()
                .icmp_imm(IntCC::Equal, call_result, 24);

            let value_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let value_imm_1 = function_builder.ins().iconst(types::I32, 1);

            function_builder.ins().brif(
                cmp_result,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_1],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            generator.context.func = func_main;

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
            "test_code_generator_import_function",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_code_generator_import_function_and_indirect_function_call() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        let pointer_t: Type = generator.module.isa().pointer_type();

        // import function 'add'
        let mut func_add_sig = generator.module.make_signature();
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.params.push(AbiParam::new(types::I32));
        func_add_sig.returns.push(AbiParam::new(types::I32));

        // import function 'get_func_add_address'
        let mut func_get_address_sig = generator.module.make_signature();
        func_get_address_sig.returns.push(AbiParam::new(pointer_t));

        let func_get_address_id = generator
            .module
            .declare_function(
                "get_func_add_address",
                Linkage::Import,
                &func_get_address_sig,
            )
            .unwrap();

        // define function "main"
        //
        // ```rust
        // fn main()->i32 {
        //     let addr = get_func_add_address();
        //     let result = indirect_call(addr, 11, 13)
        //     if result == 24 {
        //        0
        //     }else {
        //        1
        //     }
        // }
        // ```

        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let func_get_address_ref = generator
                .module
                .declare_func_in_func(func_get_address_id, &mut func_main);

            let mut function_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            let func_add_sig_ref = function_builder.import_signature(func_add_sig);

            // block_start ---> block_exit

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);

            // get the address of the function 'add'
            let inst_call_get_func_add_address =
                function_builder.ins().call(func_get_address_ref, &[]);
            let func_add_addr = function_builder.inst_results(inst_call_get_func_add_address)[0];

            // call function 'add'
            let value_imm_11 = function_builder.ins().iconst(types::I32, 11);
            let value_imm_13 = function_builder.ins().iconst(types::I32, 13);
            let inst_call_add = function_builder.ins().call_indirect(
                func_add_sig_ref,
                func_add_addr,
                &[value_imm_11, value_imm_13],
            );

            let call_result = function_builder.inst_results(inst_call_add)[0];
            let cmp_result = function_builder
                .ins()
                .icmp_imm(IntCC::Equal, call_result, 24);

            let value_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let value_imm_1 = function_builder.ins().iconst(types::I32, 1);

            function_builder.ins().brif(
                cmp_result,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_1],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            generator.context.func = func_main;

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
            "test_code_generator_import_function_and_indirect_function_call",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_code_generator_import_data() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        let pointer_t: Type = generator.module.isa().pointer_type();

        // import function 'inc_normal'
        let mut func_inc_normal_sig = generator.module.make_signature();
        func_inc_normal_sig.params.push(AbiParam::new(types::I32));

        let func_inc_normal_id = generator
            .module
            .declare_function("inc_normal", Linkage::Import, &func_inc_normal_sig)
            .unwrap();

        // import function 'read_normal'
        let mut func_read_normal_sig = generator.module.make_signature();
        func_read_normal_sig.returns.push(AbiParam::new(types::I32));

        let func_read_normal_id = generator
            .module
            .declare_function("read_normal", Linkage::Import, &func_read_normal_sig)
            .unwrap();

        // import data
        let data_normal_var_id = generator.import_data("normal_var", true, false).unwrap();

        // define function "main"
        // fn main()->i32 {
        // }

        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let func_inc_normal_ref = generator
                .module
                .declare_func_in_func(func_inc_normal_id, &mut func_main);

            let func_read_normal_ref = generator
                .module
                .declare_func_in_func(func_read_normal_id, &mut func_main);

            let gv_normal_var = generator
                .module
                .declare_data_in_func(data_normal_var_id, &mut func_main);

            let mut function_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            // block_start
            //
            // block_check0     load(normal_var)
            //                  check, assert_eq(0)
            //
            // block_check1     read_normal()
            //                  check, assert_eq(0)
            //
            // block_check2     inc_normal(11)
            //                  load(normal_var)
            //                  check, assert_eq(11)
            //
            // block_check3     read_normal()
            //                  check, assert_eq(11)
            //
            // block_check4     store(normal_var, 13)
            //                  load(normal_var)
            //                  check, assert_eq(13)
            //
            // block_check5     read_normal()
            //                  check, assert_eq(13)
            //
            // block_exit

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_check0 = function_builder.create_block();
            let block_check1 = function_builder.create_block();
            let block_check2 = function_builder.create_block();
            let block_check3 = function_builder.create_block();
            let block_check4 = function_builder.create_block();
            let block_check5 = function_builder.create_block();

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);
            function_builder.ins().jump(block_check0, &[]);

            // ins().global_value(GV) -> addr: Compute the value of global GV
            // ins().symbol_value(GV) -> addr: Compute the value of global GV, which is a symbolic value.
            // ins().tls_value(GV) -> addr: Compute the value of global GV, which is a TLS (thread local storage) value.
            // note: it seems both work.

            // bhild block_check0
            function_builder.switch_to_block(block_check0);
            let normal_var_addr = function_builder
                .ins()
                .symbol_value(pointer_t, gv_normal_var);
            let value_0 = function_builder.ins().load(
                types::I32,
                MemFlags::new(),
                normal_var_addr,
                Offset32::new(0),
            );

            let value_imm_1 = function_builder.ins().iconst(types::I32, 1);
            let cmp_result_0 = function_builder.ins().icmp_imm(IntCC::Equal, value_0, 0);

            function_builder.ins().brif(
                cmp_result_0,
                block_check1,
                &[],
                block_exit,
                &[value_imm_1],
            );

            // build block_check1
            function_builder.switch_to_block(block_check1);
            let inst_call_0 = function_builder.ins().call(func_read_normal_ref, &[]);
            let value_1 = function_builder.inst_results(inst_call_0)[0];

            let value_imm_2 = function_builder.ins().iconst(types::I32, 2);
            let cmp_result_1 = function_builder.ins().icmp_imm(IntCC::Equal, value_1, 0);

            function_builder.ins().brif(
                cmp_result_1,
                block_check2,
                &[],
                block_exit,
                &[value_imm_2],
            );

            // build block_check2
            function_builder.switch_to_block(block_check2);
            let value_imm_11 = function_builder.ins().iconst(types::I32, 11);
            function_builder
                .ins()
                .call(func_inc_normal_ref, &[value_imm_11]);

            let value_2 =
                function_builder
                    .ins()
                    .load(types::I32, MemFlags::new(), normal_var_addr, 0);
            let value_imm_3 = function_builder.ins().iconst(types::I32, 3);
            let cmp_result_2 = function_builder.ins().icmp_imm(IntCC::Equal, value_2, 11);

            function_builder.ins().brif(
                cmp_result_2,
                block_check3,
                &[],
                block_exit,
                &[value_imm_3],
            );

            // build block_check3
            function_builder.switch_to_block(block_check3);
            let inst_call_1 = function_builder.ins().call(func_read_normal_ref, &[]);
            let value_3 = function_builder.inst_results(inst_call_1)[0];

            let value_imm_4 = function_builder.ins().iconst(types::I32, 4);
            let cmp_result_3 = function_builder.ins().icmp_imm(IntCC::Equal, value_3, 11);

            function_builder.ins().brif(
                cmp_result_3,
                block_check4,
                &[],
                block_exit,
                &[value_imm_4],
            );

            // build block_check4
            function_builder.switch_to_block(block_check4);
            let value_imm_13 = function_builder.ins().iconst(types::I32, 13);
            function_builder
                .ins()
                .store(MemFlags::new(), value_imm_13, normal_var_addr, 0);

            let value_4 =
                function_builder
                    .ins()
                    .load(types::I32, MemFlags::new(), normal_var_addr, 0);
            let value_imm_5 = function_builder.ins().iconst(types::I32, 5);
            let cmp_result_4 = function_builder.ins().icmp_imm(IntCC::Equal, value_4, 13);

            function_builder.ins().brif(
                cmp_result_4,
                block_check5,
                &[],
                block_exit,
                &[value_imm_5],
            );

            // build block_check5
            function_builder.switch_to_block(block_check5);
            let inst_call_2 = function_builder.ins().call(func_read_normal_ref, &[]);
            let value_5 = function_builder.inst_results(inst_call_2)[0];

            let value_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let value_imm_6 = function_builder.ins().iconst(types::I32, 6);
            let cmp_result_5 = function_builder.ins().icmp_imm(IntCC::Equal, value_5, 13);

            function_builder.ins().brif(
                cmp_result_5,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_6],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            generator.context.func = func_main;

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
            "test_code_generator_import_data",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }

    #[test]
    fn test_code_generator_import_tls_data() {
        let mut generator = Generator::<ObjectModule>::new("main", None);

        let pointer_t: Type = generator.module.isa().pointer_type();

        // import function 'inc_tls'
        let mut func_inc_tls = generator.module.make_signature();
        func_inc_tls.params.push(AbiParam::new(types::I32));

        let func_inc_tls_id = generator
            .module
            .declare_function("inc_tls", Linkage::Import, &func_inc_tls)
            .unwrap();

        // import function 'read_tls'
        let mut func_read_tls_sig = generator.module.make_signature();
        func_read_tls_sig.returns.push(AbiParam::new(types::I32));

        let func_read_tls_id = generator
            .module
            .declare_function("read_tls", Linkage::Import, &func_read_tls_sig)
            .unwrap();

        // import data
        let data_tls_var_id = generator.import_data("tls_var", true, true).unwrap();

        // define function "main"
        //
        // fn main() -> i32 {
        // }

        let mut func_main_sig = generator.module.make_signature();
        func_main_sig.returns.push(AbiParam::new(types::I32));

        let func_main_id = generator
            .module
            .declare_function("main", Linkage::Export, &func_main_sig)
            .unwrap();

        {
            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let func_inc_tls_ref = generator
                .module
                .declare_func_in_func(func_inc_tls_id, &mut func_main);

            let func_read_tls_ref = generator
                .module
                .declare_func_in_func(func_read_tls_id, &mut func_main);

            let gv_tls_var = generator
                .module
                .declare_data_in_func(data_tls_var_id, &mut func_main);

            let mut function_builder: FunctionBuilder =
                FunctionBuilder::new(&mut func_main, &mut generator.function_builder_context);

            // block_start
            //
            // block_check0     load(tls_var)
            //                  check, assert_eq(0)
            //
            // block_check1     read_tls()
            //                  check, assert_eq(0)
            //
            // block_check2     inc_tls(11)
            //                  load(tls_var)
            //                  check, assert_eq(0)
            //
            // block_check3     read_tls()
            //                  check, assert_eq(0)
            //
            // block_check4     store(tls_var, 13)
            //                  load(tls_var)
            //                  check, assert_eq(13)
            //
            // block_check5     read_tls()
            //                  check, assert_eq(13)
            //
            // block_exit

            let block_start = function_builder.create_block();
            function_builder.append_block_params_for_function_params(block_start);

            let block_check0 = function_builder.create_block();
            let block_check1 = function_builder.create_block();
            let block_check2 = function_builder.create_block();
            let block_check3 = function_builder.create_block();
            let block_check4 = function_builder.create_block();
            let block_check5 = function_builder.create_block();

            let block_exit = function_builder.create_block();
            function_builder.append_block_params_for_function_returns(block_exit);

            // build block_start
            function_builder.switch_to_block(block_start);
            function_builder.ins().jump(block_check0, &[]);

            // build block_check0:
            // assert_eq(load(tls_var), 0) else exit(1)
            function_builder.switch_to_block(block_check0);

            let tls_var_addr = function_builder.ins().tls_value(pointer_t, gv_tls_var);
            let value_0 = function_builder.ins().load(
                types::I32,
                MemFlags::new(),
                tls_var_addr,
                Offset32::new(0),
            );

            let value_imm_1 = function_builder.ins().iconst(types::I32, 1);
            let cmp_result_0 = function_builder.ins().icmp_imm(IntCC::Equal, value_0, 0);

            function_builder.ins().brif(
                cmp_result_0,
                block_check1,
                &[],
                block_exit,
                &[value_imm_1],
            );

            // build block_check1:
            // assert_eq(read_tls(), 0) else exit(2)
            function_builder.switch_to_block(block_check1);
            let inst_call_0 = function_builder.ins().call(func_read_tls_ref, &[]);
            let value_1 = function_builder.inst_results(inst_call_0)[0];

            let value_imm_2 = function_builder.ins().iconst(types::I32, 2);
            let cmp_result_1 = function_builder.ins().icmp_imm(IntCC::Equal, value_1, 0);

            function_builder.ins().brif(
                cmp_result_1,
                block_check2,
                &[],
                block_exit,
                &[value_imm_2],
            );

            // build block_check2
            // inc_tls(11);
            // assert_eq(load(tls_var), 11) else exit(3)
            function_builder.switch_to_block(block_check2);
            let value_imm_11 = function_builder.ins().iconst(types::I32, 11);
            function_builder
                .ins()
                .call(func_inc_tls_ref, &[value_imm_11]);

            let value_2 = function_builder
                .ins()
                .load(types::I32, MemFlags::new(), tls_var_addr, 0);
            let value_imm_3 = function_builder.ins().iconst(types::I32, 3);
            let cmp_result_2 = function_builder.ins().icmp_imm(IntCC::Equal, value_2, 11);

            function_builder.ins().brif(
                cmp_result_2,
                block_check3,
                &[],
                block_exit,
                &[value_imm_3],
            );

            // build block_check3:
            // assert_eq(read_tls(), 11) else exit(4)
            function_builder.switch_to_block(block_check3);
            let inst_call_1 = function_builder.ins().call(func_read_tls_ref, &[]);
            let value_3 = function_builder.inst_results(inst_call_1)[0];

            let value_imm_4 = function_builder.ins().iconst(types::I32, 4);
            let cmp_result_3 = function_builder.ins().icmp_imm(IntCC::Equal, value_3, 11);

            function_builder.ins().brif(
                cmp_result_3,
                block_check4,
                &[],
                block_exit,
                &[value_imm_4],
            );

            // build block_check4:
            // store(tls_var, 13)
            // assert_eq(load(tls_var), 13) else exit(5)
            function_builder.switch_to_block(block_check4);
            let value_imm_13 = function_builder.ins().iconst(types::I32, 13);
            function_builder
                .ins()
                .store(MemFlags::new(), value_imm_13, tls_var_addr, 0);

            let value_4 = function_builder
                .ins()
                .load(types::I32, MemFlags::new(), tls_var_addr, 0);
            let value_imm_5 = function_builder.ins().iconst(types::I32, 5);
            let cmp_result_4 = function_builder.ins().icmp_imm(IntCC::Equal, value_4, 13);

            function_builder.ins().brif(
                cmp_result_4,
                block_check5,
                &[],
                block_exit,
                &[value_imm_5],
            );

            // build block_check5:
            // assert_eq(read_tls(), 13) else exit(6)
            function_builder.switch_to_block(block_check5);
            let inst_call_2 = function_builder.ins().call(func_read_tls_ref, &[]);
            let value_5 = function_builder.inst_results(inst_call_2)[0];

            let value_imm_0 = function_builder.ins().iconst(types::I32, 0);
            let value_imm_6 = function_builder.ins().iconst(types::I32, 6);
            let cmp_result_5 = function_builder.ins().icmp_imm(IntCC::Equal, value_5, 13);

            function_builder.ins().brif(
                cmp_result_5,
                block_exit,
                &[value_imm_0],
                block_exit,
                &[value_imm_6],
            );

            // build block_exit
            function_builder.switch_to_block(block_exit);

            let exit_code_value = function_builder.block_params(block_exit)[0];
            function_builder.ins().return_(&[exit_code_value]);

            // all blocks are finish
            function_builder.seal_all_blocks();
            function_builder.finalize();

            println!("{}", func_main.display());

            generator.context.func = func_main;

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
            "test_code_generator_import_tls_data",
            false,
        );

        assert_eq!(exit_code_opt, Some(0));
    }
}
