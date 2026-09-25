# Interactive applications

Status: design direction; not normative
Line: [Vibra 4](README.md#vibra-4--interactive-applications); GPU work on the
[horizon](README.md#horizon)
Prerequisites: [processes](concurrency.md),
[effect-row polymorphism](foundations.md#effect-row-polymorphism),
[process-owned resources](foundations.md#resources-and-handles)

## Design choice: model-view-update over processes

A user interface is a long-running process that receives events and redraws.
Vibra already has every piece of the Elm architecture (model-view-update):

- the **model** is an immutable nominal value;
- **update** is a pure function from model and message to a new model and a
  list of commands;
- **view** is a pure function from model to a UI tree value; and
- the app runtime is a process whose receive loop feeds events to `update`
  and hands the result of `view` to the host.

Imperative widget toolkits, retained mutable scene graphs, and reactive signal
systems each need mutable shared state or hidden effects, and Vibra has
neither. Model-view-update also gives agents the properties they need most:
`update` and `view` are pure, so they are unit-testable, verifiable,
replayable from a message log, and queryable as data.

## Shape of an application

```vibra
(deftype todo-msg
  (enum
    typed str
    add void
    remove u64)
  visibility: @public)

(deftype todo (record items (array str) draft str)
  visibility: @public
  (impl (app todo-msg)
    (defn update (model self message todo-msg) (tuple self (array (command todo-msg)))
      ...)
    (defn view (model self) (ui.node todo-msg)
      (ui.column
        (ui.text-input "New item" (model @draft) on-change: todo-msg.typed)
        (ui.button "Add" (todo-msg.add void))
        (ui.list (model @items) render: item-row)))))
```

- `app` is a standard interface parameterized by the message type and, through
  effect-row polymorphism, by the row of the commands it may issue.
- `(ui.node msg)` is a standard nominal UI tree whose event handlers produce
  `msg` values. Handlers are data, and a handler never runs effects.
- A command is a value: `command.none`, `command.send` to a pid,
  `command.perform`, which runs an effectful function in a worker process and
  maps its result to a message, and `command.batch`. Effectful work therefore
  happens in ordinary processes with ordinary checked rows, and the UI process
  stays pure apart from rendering.
- Subscriptions (timers, animation frames, window resize, external messages)
  are declared by an optional `subscriptions` member returning a value that the
  runtime diffs, as in Elm.
- A new `@app` target kind names the application type and its initial model.
  Its `effects` array covers the UI roots and every row its commands can carry.

## UI tree

- **Accessibility is enforced by types.** Every interactive node takes its
  accessible name as a required positional operand (`ui.button "Add" ...`), so
  an unlabeled control cannot be written. Roles, states, and focus order are
  part of the tree, and the accessibility tree is derived from it, never
  maintained separately.
- **Stable identity.** List children carry explicit keys so the host can
  reconcile efficiently. Keys are ordinary values of a `hashable` type.
- **Layout** uses a small, specified model (stacks, flex rows and columns,
  grids, and fixed boxes). A deterministic reference layout engine, using a
  reference text-measurement model, runs in the interpreter so that layout is
  testable without a display.
- **Styling** uses typed design tokens (colors, spacing, typography) rather
  than stylesheet strings.

## Hosts

The same UI tree renders on several hosts. Each host is a closed,
toolchain-owned registry with its own effect roots:

| Order | Host | Why this order |
| --- | --- | --- |
| 1 | Terminal | Cheapest to build, fits Vibra's CLI audience, and drives the headless test design |
| 2 | Browser | Wasm's native environment. The host maps the tree to DOM nodes and events |
| 3 | Desktop | The Vibra host runtime owns a native window and renderer |

Nodes a host cannot express (for example a canvas in a terminal) are reported
statically when a target selects that host, not at run time.

For the desktop host, the main choice is between a custom GPU renderer in
the host runtime and a system webview that reuses the browser host. The
webview is faster to ship. The custom renderer gives consistent layout and
input semantics. The decision belongs to the line 4 specification and needs
prototypes of both.

## 2D graphics

- `ui.canvas` holds an immutable **draw list**: paths, fills, strokes, text,
  images, clips, and transforms as ordinary values. The host rasterizes it.
- Animation is a subscription to frame messages carrying virtual-time
  timestamps. `update` computes the next model, and `view` computes the next
  draw list. Tests can step frames deterministically.
- Images and fonts are **assets** declared in `project.vibon`, hashed into the
  build like source. They are referenced by identity, never read through a
  filesystem effect, so an app needs no `fs.read` to show its own icon.

## Testing and agent access

- **Headless driver.** A test starts an app without a display, acts through
  the accessibility tree (`press` the node named "Add", `type` into the node
  named "New item"), and asserts on the model, the UI tree, or the reference
  layout. This is deterministic and runs under the simulation scheduler, so
  commands and worker processes are covered too.
- **Snapshots** of the UI tree and draw lists are canonical VIBON. Pixel
  screenshots are optional, host-specific evidence and are outside parity.
- **Replay.** Because `update` is pure, a recorded message log reproduces any
  session exactly. That enables time-travel debugging and turns field bug
  reports into regression tests.
- **Live query.** With `--allow-run`, an MCP tool can return a running app's
  current UI tree and model, and drive it through the accessibility tree. An
  agent can then test and debug an interface without screenshots.

## Parity statement

The interpreter/Wasm parity contract extends to models, messages, commands,
UI trees, draw lists, and reference layout. Rasterized pixels, platform font
rendering, and input-device timing are host outputs, in the same way that v1
treats terminal rendering of stdout bytes.

## GPU work (horizon)

GPUs cannot run Wasm, and pure Vibra is a good fit for kernels. A later track
could define a **shader subset**: pure functions over fixed-size numeric types
and vectors, no recursion except tail recursion (lowered to loops), no
collections of dynamic size, and no effects. The subset would be checked by
the ordinary type checker plus a subset rule, and lowered to a GPU shading
language such as WGSL. The same subset would serve both custom canvas shaders
and GPU-backed `par` compute. It stays on the horizon until 2D graphics and
`par` show which use arrives first.

## Open questions

- Whether `update` should be allowed a small, fixed effect row (for example
  logging) or stay strictly pure. Strictly pure is the recommended starting
  point.
- How text input with composition (input method editors) and rich text fit
  the pure model without leaking host state.
- Whether the terminal host should share the full layout model, or use a
  restricted subset with its own static diagnostics.
