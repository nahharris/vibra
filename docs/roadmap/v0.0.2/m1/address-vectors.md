# M1 independent checked component address vectors

These fixed vectors cover a complete simple function definition, self
recursion, mutual recursion, a dependency between separate components, and a
trusted registry reference. The `simple` vector is a complete
`@checked-definition.v1` function projection for the source below. The other
four cases are explicitly SCC wire/address fixtures: their compact synthetic
definition trees isolate SCC member ordering and target encoding, and are not
valid accepted source projections or examples of persisted semantic
definitions. Each frame below is the complete canonical UTF-8 VIBON component
payload, including its one final LF. The component address is
`sha256:` plus the lowercase SHA-256 of
`vibra-codebase-component-v1\0 || frame-bytes`. Member addresses are
`sha256:` plus the lowercase SHA-256 of
`vibra-codebase-object-v1\0 || component-digest-bytes || ordinal-u64be`.

The frame spellings and expected hashes were authored independently of
`encode_candidates`. A standalone Python reference calculation assembled the
literal frame bytes and used `hashlib.sha256`; the Rust vector test repeats the
domain framing directly and compares the encoder's complete payload, component
digest, component address, and member addresses with these constants. A second
Rust test imports a temporary project containing the simple source and compares
the actual source projection's frame, digest, component address, and object
address against the same fixed values.

| Vector | Component address | Member object address(es), in frame order |
| --- | --- | --- |
| Complete simple `defn` projection | `sha256:007833aa1b06fbade3d0e75485111bc6ebc39fbe1859f4959ce2161feac3aa03` | `sha256:e961613b65f10343533506c1add52cdb85098bab87c31fcfbf1958ce3cd95613` |
| SCC wire fixture: self-recursive `loop` | `sha256:8130d40260251f9dbfd50e21c3472a85051ab965d8580f70fda4b8291787e64d` | `sha256:cf09809d2ed35d14728846c1f67bc028be7582026f736470fe2e42ccb03192e0` |
| SCC wire fixture: mutual `alpha` / `beta` | `sha256:5410603ea9a48013bfb58a5690772dc3319a9a0b8b80ddec265d523337f304b3` | `alpha`: `sha256:3c04ec7243737de86c0594671c76dab58fa04fe36ee85a94545b94a9c61725b7`; `beta`: `sha256:0398214f07198493243573b82e17ec590a2311d8fcf96eda5edf105ba66e8860` |
| SCC wire fixture: external dependency, `z-library` | `sha256:962b4e74a3342bf941323d295a287021eff4712cafe143b7684d81b9b4256e80` | `sha256:3a3a15f1cb4ccfb35d934db447887e6694a3fcce03df87384b2894f07658a956` |
| SCC wire fixture: external dependency, `a-client` | `sha256:415658873a6bd2644666d1cb1cb05004f612f92f82787b5d658160c794b3dc48` | `sha256:bb49699bb68f60e7a38c8cfa82588baeaf0150e6e15fde6618e8ee6478b72387` |
| SCC wire fixture: trusted host registry reference | `sha256:732e7da21f6e3811f918f21eccc1f7c825383e11201197d5ad98f1a6ebb58ec7` | `sha256:5d9f91cb7e1b8c1cfdfee99a45cffb6c2d232d6a9a404ee53476bbe9583d8bf8` |

The simple source is `(defn simple () u32 7)`. Its checked IR body is a
single-expression sequence containing the `u32` literal. The projection also
retains the complete function part, attributes, checked signature and effects,
and explicit absent lowered/native/implementation/effect-owner fields.

The external-dependency case encodes the library component first. Its object
address is then included as an `@object` target in the client component. The
trusted-reference case uses the checked-in host registry snapshot fingerprint
`sha256:7467d79727468361e01e8a44289492103b6a482078624662c69a7999b09ef4e4`
for `@host:io.stdout-text`.

## Simple declaration

Source:

~~~vib
(defn simple () u32 7)
~~~

Complete projected component frame:

~~~vibon
(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.simple" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "function-part") (record kind: @list items: (array (record kind: @atom value: "name") (record kind: @string value: "simple"))) (record kind: @list items: (array (record kind: @atom value: "attributes") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "body") (record kind: @list items: (array (record kind: @atom value: "checked-function") (record kind: @list items: (array (record kind: @atom value: "signature") (record kind: @list items: (array (record kind: @atom value: "signature") (record kind: @list items: (array (record kind: @atom value: "positional") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "labelled") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "variadic") (record kind: @list items: (array (record kind: @atom value: "absent"))))) (record kind: @list items: (array (record kind: @atom value: "result") (record kind: @list items: (array (record kind: @atom value: "primitive") (record kind: @list items: (array (record kind: @atom value: "name") (record kind: @atom value: "u32"))))))) (record kind: @list items: (array (record kind: @atom value: "effects") (record kind: @list items: (array (record kind: @atom value: "effects") (record kind: @list items: (array (record kind: @atom value: "roots") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "tail") (record kind: @atom value: "none"))))))))))) (record kind: @list items: (array (record kind: @atom value: "body") (record kind: @list items: (array (record kind: @atom value: "expr") (record kind: @atom value: "sequence") (record kind: @list items: (array (record kind: @atom value: "expressions") (record kind: @list items: (array (record kind: @list items: (array (record kind: @atom value: "expr") (record kind: @atom value: "literal") (record kind: @list items: (array (record kind: @atom value: "value") (record kind: @integer type: @u32 value: "7"))))))))))))) (record kind: @list items: (array (record kind: @atom value: "lowered-body") (record kind: @void))) (record kind: @list items: (array (record kind: @atom value: "native-body") (record kind: @void))) (record kind: @list items: (array (record kind: @atom value: "implements") (record kind: @void))) (record kind: @list items: (array (record kind: @atom value: "effect-owner") (record kind: @void)))))))))) references: (array))))
~~~

## SCC wire fixture: self-recursive object

~~~vibon
(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.loop" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.loop")))) references: (array (record part: (array) kind: @call target: (record kind: @component-member index: 0u64))))))
~~~

## SCC wire fixture: mutually recursive component

~~~vibon
(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.alpha" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.beta")))) references: (array (record part: (array) kind: @call target: (record kind: @component-member index: 1u64)))) (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.beta" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.alpha")))) references: (array (record part: (array) kind: @call target: (record kind: @component-member index: 0u64))))))
~~~

## SCC wire fixture: separate external dependency

The dependency component is first in canonical dependency order:

~~~vibon
(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.z-library" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "body") (record kind: @integer type: @u32 value: "9")))) references: (array))))
~~~

The client frame refers to that independently addressed dependency object:

~~~vibon
(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.a-client" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.z-library")))) references: (array (record part: (array) kind: @call target: (record kind: @object address: "sha256:3a3a15f1cb4ccfb35d934db447887e6694a3fcce03df87384b2894f07658a956"))))))
~~~

## SCC wire fixture: trusted registry reference

~~~vibon
(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.external-call" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@host:io.stdout-text")))) references: (array (record part: (array) kind: @call target: (record kind: @trusted-external identity: "@host:io.stdout-text" registry: "sha256:7467d79727468361e01e8a44289492103b6a482078624662c69a7999b09ef4e4"))))))
~~~
