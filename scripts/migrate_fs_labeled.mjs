import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const p = path.join(__dirname, "..", "stdlib", "fs.vibra");
let s = fs.readFileSync(p, "utf8");

function sub(re, repl) {
  s = s.replace(re, repl);
}

sub(
  /(\n      )\$function:\n        args:\n          self: \$self\n        return:/g,
  "$1$function: $self\n      return:",
);
sub(
  /(\n        )\$function:\n          args:\n            self: \$self\n          return:/g,
  "$1$function: $self\n          return:",
);
for (const extra of [
  "start:",
  "segment:",
  "b:",
  "len:",
  "pos:",
  "n:",
  "whence:",
  "mode:",
  "off:",
]) {
  sub(
    new RegExp(
      `(\\n        )\\$function:\\n          args:\\n            self: \\$self\\n            ${extra.replace(":", "\\:")}`,
      "g",
    ),
    `$1$function: $self\n          args:\n            ${extra}`,
  );
}

sub(
  /^(\s{2})\$function:\n        args:\n          p: \$path\n          grant:/gm,
  "$1$function: $path\n    args:\n      grant:",
);
sub(
  /^(\s{2})\$function:\n        args:\n          p: \$path\n          read-grant:/gm,
  "$1$function: $path\n    args:\n      read-grant:",
);
sub(
  /^(\s{2})\$function:\n        args:\n          grant: \$security\.fs-read-grant\n          p:/gm,
  "$1$function: $security.fs-read-grant\n    args:\n      p:",
);
sub(
  /^(\s{2})\$function:\n        args:\n          grant: \$security\.fs-write-grant\n          p:/gm,
  "$1$function: $security.fs-write-grant\n    args:\n      p:",
);

s = s.replaceAll("- $args.p\n", "- $args.subject\n");

fs.writeFileSync(p, s);
console.log("wrote", p);
