//! The link command is a value: a pure function of the toolchain and three
//! paths, equal to the command the feasibility record observed to link
//! (`docs/roadmap/v0.0.3/m1/feasibility.md`, "The link and run").

use std::ffi::OsString;
use std::path::PathBuf;

use vibra_build::{
    LinkCommand, SYSTEM_LIBRARIES, Toolchain, link_command, link_environment,
    parse_installation_path, parse_kits_root,
};

fn toolchain() -> Toolchain {
    Toolchain {
        link: PathBuf::from("<LINK>"),
        library_dirs: vec![
            PathBuf::from("<MSVC-LIB-X64>"),
            PathBuf::from("<SDK-UCRT-X64>"),
            PathBuf::from("<SDK-UM-X64>"),
        ],
        msvc_version: "14.43.34808".to_owned(),
        sdk_version: "10.0.22621.0".to_owned(),
    }
}

fn strings(command: &LinkCommand) -> Vec<String> {
    command
        .args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_link_command_is_the_recorded_literal_list() {
    let command = link_command(
        &toolchain(),
        &PathBuf::from("<OBJECT>"),
        &PathBuf::from("<ARCHIVE>"),
        &PathBuf::from("<OUT>"),
    );
    assert_eq!(command.program, PathBuf::from("<LINK>"));
    assert_eq!(
        strings(&command),
        [
            "/NOLOGO",
            "/MACHINE:X64",
            "/SUBSYSTEM:CONSOLE",
            "/OUT:<OUT>",
            "<OBJECT>",
            "<ARCHIVE>",
            "/LIBPATH:<MSVC-LIB-X64>",
            "/LIBPATH:<SDK-UCRT-X64>",
            "/LIBPATH:<SDK-UM-X64>",
            "kernel32.lib",
            "ntdll.lib",
            "userenv.lib",
            "ws2_32.lib",
            "dbghelp.lib",
            "/defaultlib:msvcrt",
        ],
        "{command}"
    );
    assert_eq!(SYSTEM_LIBRARIES.len(), 6);
}

#[test]
fn the_command_is_a_pure_function_of_its_inputs() {
    let build = || {
        link_command(
            &toolchain(),
            &PathBuf::from("a.obj"),
            &PathBuf::from("b.lib"),
            &PathBuf::from("c.exe"),
        )
    };
    assert_eq!(build(), build());
}

#[test]
fn an_argument_with_spaces_or_non_ascii_stays_one_element() {
    let output = PathBuf::from("out dir/sa\u{ed}da \u{fc}n\u{ef}.exe");
    let command = link_command(
        &toolchain(),
        &PathBuf::from("w ork/a.obj"),
        &PathBuf::from("b.lib"),
        &output,
    );
    assert!(
        command
            .args
            .contains(&OsString::from(format!("/OUT:{}", output.display())))
    );
    assert!(command.args.contains(&OsString::from("w ork/a.obj")));
    // The text form quotes what has a space, to show the boundaries.
    let text = command.to_string();
    assert!(
        text.contains("\"/OUT:out dir/sa\u{ed}da \u{fc}n\u{ef}.exe\""),
        "{text}"
    );
    assert!(text.contains("\"w ork/a.obj\""), "{text}");
    assert!(text.starts_with("<LINK> /NOLOGO"), "{text}");
}

#[test]
fn the_linker_environment_is_the_temporary_directory_only() {
    let work = PathBuf::from("work dir");
    assert_eq!(
        link_environment(&work),
        [(OsString::from("TMP"), OsString::from("work dir"))]
    );
}

#[test]
fn vswhere_and_reg_output_are_read_as_documented() {
    assert_eq!(
        parse_installation_path(
            "\r\nC:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\r\n"
        ),
        Some(PathBuf::from(
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools"
        ))
    );
    assert_eq!(parse_installation_path("  \r\n"), None);
    let query = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows Kits\\Installed Roots\r\n    KitsRoot10    REG_SZ    C:\\Program Files (x86)\\Windows Kits\\10\\\r\n\r\n";
    assert_eq!(
        parse_kits_root(query),
        Some(PathBuf::from("C:\\Program Files (x86)\\Windows Kits\\10\\"))
    );
    assert_eq!(
        parse_kits_root("ERROR: The system was unable to find"),
        None
    );
}
