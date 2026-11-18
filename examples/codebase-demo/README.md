# Checked codebase demo

This package has two mutually recursive public functions, an importing binary,
and checked test bodies. M1 import checks every body without executing the
binary, tests, or host effects.

Run the following PowerShell commands from the repository root. Use a fresh
copy under the ignored `tmp/` directory so the demonstration can change source
and create its store without editing this example.

```powershell
New-Item -ItemType Directory -Force tmp | Out-Null
Copy-Item -Recurse examples/codebase-demo tmp/codebase-demo
$demo = (Resolve-Path tmp/codebase-demo).Path
cargo run --locked -p vibra-cli -- --workspace $demo codebase init
$first = cargo run --quiet --locked -p vibra-cli -- --format json --workspace $demo codebase import --expect absent | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'First import failed' }
$oldRevision = $first.payload.revision
$entity = '(record kind: @declaration identity: "@codebase-demo@0.1.0/math.parity.even" entity-kind: @function)'
$old = cargo run --quiet --locked -p vibra-cli -- --format json --workspace $demo codebase lookup --revision $oldRevision $entity | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Old lookup failed' }

$module = Join-Path $demo 'src/math/parity.vib'
$text = [IO.File]::ReadAllText($module).Replace('0u32 true', '0u32 false')
[IO.File]::WriteAllText($module, $text, [Text.UTF8Encoding]::new($false))
$second = cargo run --quiet --locked -p vibra-cli -- --format json --workspace $demo codebase import --expect $oldRevision | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Second import failed' }
$newRevision = $second.payload.revision
$new = cargo run --quiet --locked -p vibra-cli -- --format json --workspace $demo codebase lookup --revision $newRevision $entity | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'New lookup failed' }

cargo run --locked -p vibra-cli -- --workspace $demo codebase read --revision $oldRevision $old.payload.object
cargo run --locked -p vibra-cli -- --workspace $demo codebase read --revision $newRevision $new.payload.object
cargo run --locked -p vibra-cli -- --workspace $demo codebase verify
cargo run --locked -p vibra-cli -- --workspace $demo codebase backup snapshot.sqlite
cargo run --locked -p vibra-cli -- --workspace $demo codebase restore snapshot.sqlite --store .vibra/restored.sqlite
cargo run --locked -p vibra-cli -- --workspace $demo codebase head --store .vibra/restored.sqlite
```

Each invocation is a new process. Both revisions remain readable after reopen;
the two recursive definitions and their callers change addresses together.
The source edit deliberately changes the even base case: import checks the
existing assertions but does not run them. A stale import using `$oldRevision`
again must fail without moving the head.

Export is explicit and requires an absent destination. Preview and apply use
the same pinned revision and exact plan address:

```powershell
$preview = cargo run --quiet --locked -p vibra-cli -- --format json --workspace $demo codebase export --revision $oldRevision --destination exported | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Export preview failed' }
cargo run --locked -p vibra-cli -- --workspace $demo codebase export --revision $oldRevision --destination exported --write --plan $preview.payload.plan
```

The exported tree contains the old exact project and `.vib` bytes. Existing
destinations are preserved and refused. All paths are relative to this trusted
local workspace; see the [M1 contract](../../docs/spec/05-tooling.md#m1-codebase-commands).
