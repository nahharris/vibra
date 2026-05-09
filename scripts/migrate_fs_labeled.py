"""One-off: migrate stdlib/fs.vibra nested $function to labeled form."""
import pathlib
import re

p = pathlib.Path(__file__).resolve().parent.parent / "stdlib" / "fs.vibra"
text = p.read_text(encoding="utf-8")

def sub(pat, repl, s, flags=0):
    return re.sub(pat, repl, s, flags=flags)

# path.* defs: 6-space base, 8-space $function
s = text
s = sub(
    r"(\n      )\$function:\n        args:\n          self: \$self\n        return:",
    r"\1$function: $self\n      return:",
    s,
)
s = sub(
    r"(\n        )\$function:\n          args:\n            self: \$self\n          return:",
    r"\1$function: $self\n          return:",
    s,
)
# slice / variants with extra args after self
for extra in (
    "start:",
    "segment:",
    "b:",
    "len:",
    "pos:",
    "n:",
    "whence:",
    "mode:",
    "off:",
):
    s = sub(
        rf"(\n        )\$function:\n          args:\n            self: \$self\n            {extra}",
        rf"\1$function: $self\n          args:\n            {extra}",
        s,
    )

# Module-level open-* (2 spaces + $function at col 2)
s = sub(
    r"^(\s{2})\$function:\n        args:\n          p: \$path\n          grant:",
    r"\1$function: $path\n    args:\n      grant:",
    s,
    flags=re.M,
)
s = sub(
    r"^(\s{2})\$function:\n        args:\n          p: \$path\n          read-grant:",
    r"\1$function: $path\n    args:\n      read-grant:",
    s,
    flags=re.M,
)
s = sub(
    r"^(\s{2})\$function:\n        args:\n          grant: \$security\.fs-read-grant\n          p:",
    r"\1$function: $security.fs-read-grant\n    args:\n      p:",
    s,
    flags=re.M,
)
s = sub(
    r"^(\s{2})\$function:\n        args:\n          grant: \$security\.fs-write-grant\n          p:",
    r"\1$function: $security.fs-write-grant\n    args:\n      p:",
    s,
    flags=re.M,
)

# Replace wasm $args.p with $args.subject for path-primary fns (heuristic)
s = s.replace("- $args.p\n", "- $args.subject\n")

p.write_text(s, encoding="utf-8")
print("wrote", p)
