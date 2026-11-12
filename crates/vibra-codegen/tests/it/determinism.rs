//! Emission depends on (program, target, inputs) and on nothing else.

use std::process::Command;

use vibra_codegen::{EmitError, Inputs, Target, emit};
use vibra_exec::verified;

use crate::programs;
use crate::{TARGETS, emitted, inputs};

const CHILD: &str = "VIBRA_CODEGEN_DETERMINISM_CHILD";
const BEGIN: &str = "EMISSIONS-BEGIN";
const END: &str = "EMISSIONS-END";

/// FNV-1a, 64 bits: a digest that is the same in every process.
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// One line for each program and target: its name, the length of its object,
/// and the digest of its bytes.
fn emissions() -> String {
    let mut text = String::new();
    for (name, program) in programs::all() {
        for target in TARGETS {
            let object = emitted(&program, &target);
            text.push_str(&format!(
                "{name} {} {} {:016x}\n",
                target.triple(),
                object.as_bytes().len(),
                digest(object.as_bytes())
            ));
        }
    }
    text
}

#[test]
fn emission_is_byte_identical_across_runs() {
    for (name, program) in programs::all() {
        for target in TARGETS {
            let first = emitted(&program, &target);
            // Another emission in between: nothing is shared between runs.
            for (_, other) in programs::all() {
                emitted(&other, &target);
            }
            let second = emitted(&program.clone(), &target);
            assert_eq!(first, second, "{name} for {}", target.triple());
            assert!(!first.as_bytes().is_empty());
        }
    }
}

#[test]
fn emission_is_byte_identical_across_processes() {
    if std::env::var_os(CHILD).is_some() {
        println!("{BEGIN}\n{}{END}", emissions());
        return;
    }
    let exe = std::env::current_exe().expect("the test binary");
    let output = Command::new(exe)
        .args([
            "--exact",
            "determinism::emission_is_byte_identical_across_processes",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .expect("a second process");
    assert!(output.status.success(), "{output:?}");
    let printed = String::from_utf8(output.stdout).expect("text");
    let between = printed
        .split_once(BEGIN)
        .and_then(|(_, rest)| rest.split_once(END))
        .map(|(text, _)| text.trim().to_owned())
        .expect("the child printed its emissions");
    assert_eq!(between, emissions().trim());
}

#[test]
fn the_inputs_reach_the_object_and_only_they_and_the_program_and_target_do() {
    let program = programs::demo(5, false);
    let proof = verified(&program).expect("verified");
    let base = inputs(&program);
    let object = |inputs: Inputs, target: &Target| {
        emit(proof, target, &inputs)
            .expect("an object")
            .into_bytes()
    };
    let reference = object(base, &Target::WINDOWS_X64);
    assert_eq!(reference, object(base, &Target::WINDOWS_X64));
    let outcome = Inputs {
        entry_outcome: base.entry_outcome + 1,
        ..base
    };
    let minor = Inputs {
        registry_minor: base.registry_minor + 1,
        ..base
    };
    assert_ne!(reference, object(outcome, &Target::WINDOWS_X64));
    assert_ne!(reference, object(minor, &Target::WINDOWS_X64));
    assert_ne!(reference, object(base, &Target::LINUX_X64));
}

#[test]
fn an_unsupported_triple_is_an_error() {
    for triple in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "i686-pc-windows-msvc",
        "",
    ] {
        assert_eq!(
            Target::new(triple),
            Err(EmitError::UnsupportedTarget(triple.to_owned()))
        );
    }
    for triple in vibra_codegen::SUPPORTED_TRIPLES {
        assert_eq!(Target::new(triple).expect("supported").triple(), *triple);
    }
}
