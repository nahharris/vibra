import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const p = path.join(__dirname, "..", "tests", "integration.rs");
let s = fs.readFileSync(p, "utf8");
s = s.replace(/\r\n/g, "\n");

function rep(a, b) {
  const n = s.split(a).length - 1;
  if (n) s = s.split(a).join(b);
  return n;
}

// Most common: module-level main with void args
rep(
  `  $function:\n    args: $void\n    return: $void\n    do:`,
  `  $function: $void\n  return: $void\n  do:`,
);

rep(
  `  $function:\n    args:\n      grants: $security.grants\n    return: $void\n    do:`,
  `  $function: $security.grants\n  return: $void\n  do:`,
);

// Generic identity definition (many tests)
rep(
  `identity:\n  $function:\n    args:\n      x: $t\n    return: $t\n    do:\n      - $return: $args.x`,
  `identity:\n  $function: $t\n  return: $t\n  do:\n      - $return: $args.subject`,
);

// Generic call sites
rep(`$identity:\n              t: $int64\n              x: 7`, `$identity: 7\n              t: $int64`);
rep(`$identity:\n          x: 7`, `$identity: 7`);
rep(
  `$identity:\n          t: $int64\n          x: 7\n          q: 1`,
  `$identity: 7\n          t: $int64\n          q: 1`,
);
rep(`$identity:\n          t: $int64\n          x: "hi"`, `$identity: "hi"\n          t: $int64`);

// Iface record call forms
rep(`{ $display.fmt: { x: $b } }`, `{ $display.fmt: $b }`);
rep(`{ $display.fmt: { x: 7 } }`, `{ $display.fmt: 7 }`);
rep(`{ $display.fmt: { x: $args.x } }`, `{ $display.fmt: $args.subject }`);

rep(
  `fmt-via-bound:\n  $function:\n    args:\n      x: $t\n    return: $str\n    do:\n      - $let:\n          s: { $display.fmt: $args.subject }\n      - $return: $s`,
  `fmt-via-bound:\n  $function: $t\n  return: $str\n  do:\n      - $let:\n          s: { $display.fmt: $args.subject }\n      - $return: $s`,
);

rep(
  `        $function:\n          args:\n            x: $self\n          return: $str\n          do:\n            - $return: "boxed"`,
  `        $function: $self\n        return: $str\n        do:\n            - $return: "boxed"`,
);

rep(
  `        $function:\n          args:\n            x: $int64\n          return: $void\n          do:\n            - $let:\n                unused: $args.x`,
  `        $function: $int64\n        return: $void\n        do:\n            - $let:\n                unused: $args.subject`,
);

rep(`- $from-iface.from: { x: 5 }`, `- $from-iface.from: 5`);

rep(
  `bad:\n  $function:\n    args:\n      x: $int64\n    return: $int64\n    do:\n      - $io.println: "nope"`,
  `bad:\n  $function: $int64\n  return: $int64\n  do:\n      - $io.println: "nope"`,
);

// After main uses $function: $security.grants, body should use $args.subject not $args.grants
s = s.replaceAll(`$args.grants`, `$args.subject`);

rep(`$fs.writable.write-string:\n          self: $f\n          s: "nope"`, `$fs.writable.write-string: $f\n          s: "nope"`);

rep(
  `  $function:\n    args:\n      grants: $sec.grants\n    return: $void`,
  `  $function: $sec.grants\n  return: $void`,
);

rep(
  `doThing:\n  $function:\n    args:\n      BadArg: $str\n    return: $void`,
  `doThing:\n  $function: $str\n  return: $void`,
);
rep(`- $args.BadArg`, `- $args.subject`);

rep(
  `accepts-int32:\n  $function:\n    args:\n      x: $int32\n    return: $void`,
  `accepts-int32:\n  $function: $int32\n  return: $void`,
);
rep(
  `accepts-float32:\n  $function:\n    args:\n      x: $float32\n    return: $void`,
  `accepts-float32:\n  $function: $float32\n  return: $void`,
);

rep(
  `take-meter:\n  $function:\n    args:\n      value: $meter\n    return: $void`,
  `take-meter:\n  $function: $meter\n  return: $void`,
);

fs.writeFileSync(p, s);
console.log("wrote", p);
