//! MVP lowering: single `$io.println: "..."` in `main` backed by stdlib `println` + `$wasm` host `println`.

use crate::load::{map_get_str, LoadedProgram};
use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub fn extract_print_message(program: &LoadedProgram) -> Result<String> {
    let entry_map = program
        .modules
        .get(&program.entry)
        .context("internal: entry module not loaded")?
        .as_mapping()
        .context("entry root must be mapping")?;

    let mut import_aliases: HashMap<String, PathBuf> = HashMap::new();
    let parent = program
        .entry
        .parent()
        .context("entry path has no parent")?;

    for (k, v) in entry_map {
        let key = k
            .as_str()
            .context("module keys must be strings")?;
        if key.starts_with('-') {
            continue;
        }
        if let Some(sub) = v.as_mapping() {
            if let Some(imp) = map_get_str(sub, "$import") {
                let s = imp
                    .as_str()
                    .context("$import value must be string")?;
                let resolved = fs::canonicalize(parent.join(s))
                    .with_context(|| format!("resolve import alias `{key}`"))?;
                import_aliases.insert(key.to_string(), resolved);
            }
        }
    }

    let main = map_get_str(entry_map, "main").context("missing top-level `main`")?;
    let main_fn = map_get_str(
        main.as_mapping().context("`main` must be a mapping")?,
        "$function",
    )
    .context("`main` must be a `$function`")?;

    let fn_body = main_fn
        .as_mapping()
        .context("`$function` body must be mapping")?;

    let args = map_get_str(fn_body, "args").context("missing `args` on main")?;
    let args_map = args
        .as_mapping()
        .context("`args` must be a mapping (use `{}` for empty)")?;
    if !args_map.is_empty() {
        bail!("MVP: `main` must have empty `args: {{}}`");
    }

    let ret = map_get_str(fn_body, "return").context("missing `return` on main")?;
    let ret_s = ret.as_str().context("`return` must be `$void` for MVP")?;
    if ret_s != "$void" {
        bail!("MVP: `main` must have `return: $void`");
    }

    let do_seq = map_get_str(fn_body, "do").context("missing `do` on main")?;
    let steps = do_seq
        .as_sequence()
        .context("`do` must be a sequence")?;
    if steps.len() != 1 {
        bail!("MVP: `main.do` must contain exactly one statement");
    }

    let stmt = steps
        .first()
        .context("empty `do`")?
        .as_mapping()
        .context("statement must be a mapping")?;
    if stmt.len() != 1 {
        bail!("MVP: one invocation per statement");
    }
    let (call_key, arg_val) = stmt.iter().next().unwrap();
    let call = call_key
        .as_str()
        .context("invocation key must be string")?;

    let (alias, symbol) = parse_qualified_call(call)?;
    let imported_path = import_aliases
        .get(alias)
        .with_context(|| format!("unknown import alias `{alias}` in `{call}`"))?;

    let imported = program
        .modules
        .get(imported_path)
        .context("imported module missing from graph")?;

    verify_println_stub(imported, symbol)?;

    let msg = arg_val
        .as_str()
        .context("MVP: println argument must be a double-quoted string scalar")?;
    Ok(msg.to_string())
}

fn parse_qualified_call(call: &str) -> Result<(&str, &str)> {
    let rest = call.strip_prefix('$').context("call key must start with `$`")?;
    let (a, b) = rest
        .split_once('.')
        .context("MVP: expected qualified call `$alias.symbol` (e.g. `$io.println`)")?;
    if a.is_empty() || b.is_empty() {
        bail!("invalid qualified call `{call}`");
    }
    Ok((a, b))
}

fn verify_println_stub(module_root: &Value, symbol: &str) -> Result<()> {
    if symbol != "println" {
        bail!("MVP: only `println` is supported on imports, got `{symbol}`");
    }
    let map = module_root
        .as_mapping()
        .context("imported module root must be mapping")?;

    let def = map_get_str(map, symbol).context("imported module has no `println`")?;
    let fn_map = map_get_str(
        def.as_mapping().context("`println` must be mapping")?,
        "$function",
    )
    .context("`println` must be a `$function`")?;

    let body = fn_map
        .as_mapping()
        .context("`$function` value must be mapping")?;

    let args = map_get_str(body, "args").context("println: missing args")?;
    let args_m = args.as_mapping().context("println: args must be mapping")?;
    if args_m.len() != 1 {
        bail!("MVP: println must have exactly `msg: $str`");
    }
    let (pk, pv) = args_m.iter().next().unwrap();
    let pk = pk.as_str().context("arg name must be string")?;
    if pk != "msg" {
        bail!("MVP: println arg must be named `msg`");
    }
    let pt = pv.as_str().context("println `msg` must be `$str`")?;
    if pt != "$str" {
        bail!("MVP: println `msg` must be typed `$str`");
    }

    let ret = map_get_str(body, "return").context("println: missing return")?;
    if ret.as_str() != Some("$void") {
        bail!("MVP: println must return `$void`");
    }

    let do_seq = map_get_str(body, "do").context("println: missing do")?;
    let steps = do_seq.as_sequence().context("println: do must be sequence")?;
    if steps.len() != 1 {
        bail!("MVP: println body must be a single `$wasm`");
    }
    let wasm_stmt = steps[0]
        .as_mapping()
        .context("println: statement must be mapping")?;
    if wasm_stmt.len() != 1 {
        bail!("MVP: println: one key per statement");
    }
    let (wk, wv) = wasm_stmt.iter().next().unwrap();
    if wk.as_str() != Some("$wasm") {
        bail!("MVP: println must call `$wasm`");
    }
    let wm = wv.as_mapping().context("$wasm body must be mapping")?;
    let host = map_get_str(wm, "host")
        .context("$wasm: missing host")?
        .as_str()
        .context("$wasm.host must be string")?;
    if host != "println" {
        bail!("MVP: $wasm host must be `println` for this stdlib");
    }
    let wa = map_get_str(wm, "args").context("$wasm: missing args")?;
    let wa = wa.as_sequence().context("$wasm.args must be sequence")?;
    if wa.len() != 1 {
        bail!("MVP: $wasm args must be `[$args.msg]`");
    }
    let arg0 = wa[0].as_str().context("$wasm arg must be string")?;
    if arg0 != "$args.msg" {
        bail!("MVP: $wasm args must be exactly `[$args.msg]`");
    }

    Ok(())
}
