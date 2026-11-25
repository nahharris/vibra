# Aurum bibliography

Every source the Aurum notes rely on, with the key the notes cite it by.
Each entry states how far it was checked, so that a later reader knows which
claims rest on the paper and which on a summary of it:

- **read**: the paper's text was queried for the claims the notes make;
- **abstract**: only the abstract, a talk, or a secondary summary was checked;
- **recalled**: a standard reference cited from memory. Its existence and
  venue are reliable; re-read it before relying on a detail.

An identifier is given only where it was confirmed. An entry without one is
found by its title and venue.

## Interaction nets, optimal reduction, and their implementations

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| Lafont90 | Y. Lafont. *Interaction nets.* POPL 1990. | recalled | The model: agents, principal ports, local rules, strong confluence |
| Lafont97 | Y. Lafont. *Interaction combinators.* Information and Computation 137(1), 1997. | recalled | Three symbols and six rules are universal |
| Girard87 | J.-Y. Girard. *Linear logic.* Theoretical Computer Science 50, 1987. | recalled | Proof nets, the origin of interaction nets |
| Girard89 | J.-Y. Girard. *Geometry of interaction I: interpretation of System F.* Logic Colloquium '88, 1989. | recalled | Token-passing reading of a fixed net |
| Mackie95 | I. Mackie. *The geometry of interaction machine.* POPL 1995. | recalled | A machine that walks a fixed graph instead of rewriting it |
| DanosRegnier99 | V. Danos, L. Regnier. *Reversible, irreversible and optimal λ-machines.* Theoretical Computer Science 227, 1999. | recalled | Token machines and their relation to optimal reduction |
| Levy78 | J.-J. Lévy. *Réductions correctes et optimales dans le lambda-calcul.* Thèse d'État, Paris VII, 1978. | recalled | Optimality as sharing of redex families |
| Lamping90 | J. Lamping. *An algorithm for optimal lambda calculus reduction.* POPL 1990. | recalled | The first optimal sharing-graph algorithm |
| GAL92 | G. Gonthier, M. Abadi, J.-J. Lévy. *The geometry of optimal lambda reduction.* POPL 1992. | recalled | Brackets and croissants explained by linear logic |
| LawallMairson96 | J. Lawall, H. Mairson. *Optimality and inefficiency: what isn't a cost model of the lambda calculus?* ICFP 1996. | recalled | Bookkeeping work is not counted by the number of shared β-steps |
| AspertiMairson01 | A. Asperti, H. Mairson. *Parallel beta reduction is not elementary recursive.* Information and Computation 170(1), 2001 (POPL 1998). | abstract | A family of shared redexes can hide non-elementary work |
| AspertiGuerrini98 | A. Asperti, S. Guerrini. *The Optimal Implementation of Functional Programming Languages.* Cambridge University Press, 1998. | recalled | Reference text for sharing graphs |
| Asperti17 | A. Asperti. *About the efficient reduction of lambda terms.* arXiv:1701.04240, 2017. | abstract | The lower bound says little about practical efficiency |
| GuerriniSolieri17 | S. Guerrini, M. Solieri. *Is the optimal implementation inefficient? Elementarily not.* FSCD 2017. DOI 10.4230/LIPIcs.FSCD.2017.17. | abstract | Sharing overhead bounded quadratically in a restricted setting |
| Salvadori25 | Salvadori. *Δ-Nets: interaction-based system for optimal parallel λ-reduction.* arXiv:2505.20314. | read | The linear fragment is local and order-free; erasure and sharing need non-local rules and a sequential order |
| Taelin24 | V. Taelin. *HVM2: a parallel evaluator for interaction combinators.* Higher Order Company, 2024, work in progress. Distributed with the `hvm` crate as `paper/PAPER.pdf` (<https://github.com/HigherOrderCO/HVM>). | abstract | Throughput on CPU and GPU |
| LWN25 | LWN.net article on Bend and HVM, 2025. <https://lwn.net/Articles/1011803> | abstract | Bend 1 was not competitive with Haskell or OCaml |
| BendDocs | *Bend* README and guide. <https://github.com/HigherOrderCO/Bend> | abstract | Code runs in parallel unless it is inherently sequential; 24-bit numbers |
| Bend2 | Coverage of the Bend 2 release, September 2026, for example <https://akitaonrails.com/en/2026/09/19/new-ai-language-just-released-bend-2/>; see also the status report linked from [the paradigm notes](../blue-sky/paradigms.md#interaction-nets-as-the-runtime). | abstract | Bend 2 has no interaction nets at run time |
| Vine | *The Vine programming language* and its Ivy interaction-combinator VM. <https://github.com/VineLang/vine> | abstract | An active interaction-net language |
| Jiresch14 | E. Jiresch. *Towards a GPU-based implementation of interaction nets.* arXiv:1404.0076, 2014. | abstract | Available parallelism depends on the program and its run-time values |
| HMS15 | A. Hassan, I. Mackie, S. Sato. *An implementation model for interaction nets.* arXiv:1505.07164, 2015. | abstract | Compilation model for interaction nets |

## Graph-reduction and combinator hardware

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| SKIM80 | T. Clarke, P. Gladstone, C. MacLean, A. Norman. *SKIM — the S, K, I reduction machine.* LISP Conference 1980. | recalled | Early combinator hardware |
| GRIP87 | S. Peyton Jones, C. Clack, J. Salkild, M. Hardie. *GRIP — a high-performance architecture for parallel graph reduction.* FPCA 1987. | recalled | Parallel graph reduction on a multiprocessor |
| Reduceron10 | M. Naylor, C. Runciman. *The Reduceron reconfigured.* ICFP 2010. DOI 10.1145/1863543.1863556. | abstract | Template instantiation with wide memories; 96 MHz on a Virtex-5 |
| Reduceron12 | M. Naylor, C. Runciman. *The Reduceron reconfigured and re-evaluated.* Journal of Functional Programming 22(4–5), 2012. | abstract | Comparison with a conventional implementation on a soft RISC core on the same FPGA |
| Heron23 | C. Ramsay, R. Stewart. *Heron: modern hardware graph reduction.* IFL 2023. DOI 10.1145/3652561.3652564. <https://github.com/haflang/heron> | abstract | Up to six times GHC's reductions per cycle in 2% of an FPGA |
| Massimult24 | Nicklisch-Franken, Feizerakhmanov. *Massimult: a novel parallel CPU architecture based on combinator reduction.* arXiv:2412.02765, 2024. | read | Reference counting under unsynchronized parallel rewriting |
| FitchettFox26 | Fitchett, Fox. *CPU-less parallel execution of lambda calculus in digital logic.* arXiv:2601.13040, 2026. | read | A tree of λ cells in logic simulation; limits at 16 nodes |

## Dataflow, spatial, and statically scheduled machines

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| Kahn74 | G. Kahn. *The semantics of a simple language for parallel programming.* IFIP Congress 1974. | recalled | Determinacy of process networks |
| Dennis74 | J. Dennis. *First version of a data flow procedure language.* Programming Symposium, LNCS 19, 1974. | recalled | Static dataflow |
| Manchester85 | J. Gurd, C. Kirkham, I. Watson. *The Manchester prototype dataflow computer.* Communications of the ACM 28(1), 1985. | recalled | Tagged-token dataflow hardware |
| TTDA90 | Arvind, R. Nikhil. *Executing a program on the MIT tagged-token dataflow architecture.* IEEE Transactions on Computers 39(3), 1990. | recalled | The associative matching store |
| Monsoon90 | G. Papadopoulos, D. Culler. *Monsoon: an explicit token-store architecture.* ISCA 1990. | abstract | Frames replace associative matching |
| Petersen06 | A. Petersen, A. Putnam, M. Mercaldi, A. Schwerin, S. Eggers, S. Swanson, M. Oskin. *Reducing control overhead in dataflow architectures.* PACT 2006. | abstract | Dataflow executes two to four times the instructions of control flow |
| WaveScalar03 | S. Swanson, K. Michelson, A. Schwerin, M. Oskin. *WaveScalar.* MICRO 2003. | recalled | Dataflow for imperative programs with ordered memory |
| TRIPS04 | D. Burger et al. *Scaling to the end of silicon with EDGE architectures.* IEEE Computer 37(7), 2004. | recalled | Dataflow inside a block, control flow between blocks |
| SNAFU21 | G. Gobieski et al. *SNAFU: an ultra-low-power, energy-minimal CGRA-generation framework and architecture.* ISCA 2021. | recalled | Energy-minimal spatial dataflow |
| RipTide22 | G. Gobieski, S. Ghosh, M. Heule, T. Mowry, T. Nowatzki, N. Beckmann, B. Lucia. *RipTide: a programmable, energy-minimal dataflow compiler and architecture.* MICRO 2022. DOI 10.1109/MICRO56248.2022.00046. | abstract | Ordered dataflow without tags; 6.6× less energy than a von Neumann core |
| Wavelet26 | Lin, Cai, Surbatovich. *Let it flow: a formally verified compilation framework for asynchronous dataflow.* PLDI 2026. arXiv:2608.05451. | read | Verified compilation to dataflow, its source restrictions, and its proof cost |
| WaveCert23 | *WaveCert: translation validation for asynchronous dataflow programs via dynamic fractional permissions.* arXiv:2312.09326, 2023. | abstract | Translation validation as the cheaper alternative to a compiler proof |
| MADAR26 | Bergach. *MADAR: an address-free processor.* arXiv:2606.15535, 2026. | read | A statically scheduled design without a program counter, reported with its losses |
| Tascade23 | M. Orenes-Vera, E. Tureci, D. Wentzlaff, M. Martonosi. *Tascade: hardware support for atomic-free, asynchronous and efficient reduction trees.* arXiv:2311.15810, 2023. | abstract | Communication distance and load imbalance limit manycore scaling |
| SpiNNaker14 | S. Furber, F. Galluppi, S. Temple, L. Plana. *The SpiNNaker project.* Proceedings of the IEEE 102(5), 2014. | recalled | A million-core asynchronous message-passing machine |
| DallyTowles01 | W. Dally, B. Towles. *Route packets, not wires: on-chip interconnection networks.* DAC 2001. | recalled | Network-on-chip design |
| Carloni01 | L. Carloni, K. McMillan, A. Sangiovanni-Vincentelli. *Theory of latency-insensitive design.* IEEE Transactions on CAD 20(9), 2001. | recalled | Correctness independent of wire delay |
| Chapiro84 | D. Chapiro. *Globally-asynchronous locally-synchronous systems.* PhD thesis, Stanford, 1984. | recalled | GALS |

## Functional and typed hardware compilation

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| SAFL00 | A. Mycroft, R. Sharp. *A statically allocated parallel functional language.* ICALP 2000. Papers at <https://www.cl.cam.ac.uk/~am21/research/sa>. | abstract | First-order, tail recursion only, so that storage is static |
| GoS07 | D. Ghica. *Geometry of synthesis: a structured approach to VLSI design.* POPL 2007. DOI 10.1145/1190216.1190269. | abstract | Affine typing gives the game semantics a finite hardware form |
| GoS11 | D. Ghica, A. Smith, S. Singh. *Geometry of synthesis IV: compiling affine recursion into static hardware.* ICFP 2011. | abstract | Recursion unfolded in space or in time |
| Clash | C. Baaij, M. Kooijman, J. Kuper, A. Boeijink, M. Gerards. *CλaSH: structural descriptions of synchronous hardware using Haskell.* DSD 2010; and the compiler's limitations page, <https://docs.clash-lang.org/compiler-user-guide/developing-hardware/limitations.html>. | abstract | Which Haskell a circuit compiler refuses |
| Dahlia20 | R. Nigam et al. *Predictable accelerator design with time-sensitive affine types.* PLDI 2020. DOI 10.1145/3385412.3385974. arXiv:2004.04852. | abstract | Affine types for hardware resources |
| Calyx21 | R. Nigam et al. *A compiler infrastructure for accelerator generators.* ASPLOS 2021. | recalled | An intermediate language between software-like control and structure |
| Filament23 | R. Nigam et al. *Modular hardware design with timeline types.* PLDI 2023. | recalled | Timing as a type |
| WGSL | *WebGPU Shading Language.* W3C. | recalled | A mainstream target language that forbids recursion |
| Coeffects14 | T. Petricek, D. Orchard, A. Mycroft. *Coeffects: a calculus of context-dependent computation.* ICFP 2014. | recalled | What a computation requires of its context, as a typed quantity |

## Resources, ownership, and reuse

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| Perceus21 | A. Reinking, N. Xie, L. de Moura, D. Leijen. *Perceus: garbage free reference counting with reuse.* PLDI 2021 (extended: MSR-TR-2020-42). | abstract | Precise counting from a linear resource calculus; reuse in place |
| FP2 | A. Lorenzen, D. Leijen, W. Swierstra. *FP²: fully in-place functional programming.* ICFP 2023. | recalled | A fragment that needs no allocation at all |
| Beans19 | S. Ullrich, L. de Moura. *Counting immutable beans: reference counting optimized for purely functional programming.* IFL 2019 (arXiv:1908.05647). | recalled | Borrow inference and reuse in Lean 4 |
| LinearHaskell18 | J.-P. Bernardy, M. Boespflug, R. Newton, S. Peyton Jones, A. Spiwack. *Linear Haskell: practical linearity in a higher-order polymorphic language.* POPL 2018. | recalled | Multiplicities on arrows |
| QTT18 | R. Atkey. *Syntax and semantics of quantitative type theory.* LICS 2018. | recalled | Usage counts in a dependent theory |
| Idris2 | E. Brady. *Idris 2: quantitative type theory in practice.* ECOOP 2021 (arXiv:2104.00480). | recalled | Erased, linear, and unrestricted bindings in one language |
| Granule19 | D. Orchard, V.-B. Liepelt, H. Eades. *Quantitative program reasoning with graded modal types.* ICFP 2019. | recalled | Graded modalities |
| Steele09 | G. Steele. *Organizing functional code for parallel execution; or, foldl and foldr considered slightly harmful.* ICFP 2009. DOI 10.1145/1596550.1596551. | abstract | Parallelism needs associative, tree-shaped reduction |
| Bagwell01 | P. Bagwell. *Ideal hash trees.* EPFL technical report, 2001. | recalled | Hash array mapped tries |
| RRB11 | P. Bagwell, T. Rompf. *RRB-trees: efficient immutable vectors.* EPFL technical report, 2011. | recalled | A persistent sequence with cheap update and concatenation |

## Verified compilers and hardware

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| CompCert09 | X. Leroy. *Formal verification of a realistic compiler.* Communications of the ACM 52(7), 2009. | recalled | Simulation proofs for a compiler |
| CakeML14 | R. Kumar, M. Myreen, M. Norrish, S. Owens. *CakeML: a verified implementation of ML.* POPL 2014. | recalled | A verified ML compiler |
| Silver19 | A. Lööw, R. Kumar, Y. K. Tan, M. Myreen, M. Norrish, O. Abrahamsson, A. Fox. *Verified compilation on a verified processor.* PLDI 2019. DOI 10.1145/3314221.3314622. | abstract | One theorem from source to a processor, run on an FPGA |
| Lightbulb21 | A. Erbsen, S. Gruetter, J. Choi, C. Wood, A. Chlipala. *Integration verification across software and hardware for a simple embedded system.* PLDI 2021. <https://github.com/mit-plv/bedrock2> | abstract | An end-to-end theorem for one concrete program on a Kami processor |
| Kami17 | J. Choi, M. Vijayaraghavan, B. Sherman, A. Chlipala, Arvind. *Kami: a platform for high-level parametric hardware specification and its modular verification.* ICFP 2017. DOI 10.1145/3110268. | abstract | Verified rule-based hardware |
| Koika20 | T. Bourgeat, C. Pit-Claudel, A. Chlipala, Arvind. *The essence of Bluespec: a core language for rule-based hardware design.* PLDI 2020. | recalled | Rule-based hardware with a verified compiler |
| Vericert21 | Y. Herklotz, J. Pollard, N. Ramanathan, J. Wickerson. *Formal verification of high-level synthesis.* OOPSLA 2021. DOI 10.1145/3485494. | abstract | A verified C-to-Verilog tool and its performance gap. The figures the notes quote come from the authors' LATTE 2021 workshop paper, <https://capra.cs.cornell.edu/latte21/paper/3.pdf> |

## Number systems and exact arithmetic

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| Renyi57 | A. Rényi. *Representations for real numbers and their ergodic properties.* Acta Mathematica Academiae Scientiarum Hungaricae 8, 1957. | recalled | β-expansions |
| Parry60 | W. Parry. *On the β-expansions of real numbers.* Acta Mathematica Academiae Scientiarum Hungaricae 11, 1960. | recalled | Which digit strings are admissible |
| Bergman57 | G. Bergman. *A number system with an irrational base.* Mathematics Magazine 31(2), 1957. | recalled | Base φ |
| Zeckendorf72 | E. Zeckendorf. *Représentation des nombres naturels par une somme de nombres de Fibonacci ou de nombres de Lucas.* Bulletin de la Société Royale des Sciences de Liège 41, 1972. | recalled | Unique sums of non-adjacent Fibonacci numbers |
| Avizienis61 | A. Avizienis. *Signed-digit number representations for fast parallel arithmetic.* IRE Transactions on Electronic Computers EC-10, 1961. | recalled | Carry-free addition with redundant digits, base 3 and above |
| ChowRobertson78 | C. Chow, J. Robertson. *Logical design of a redundant binary adder.* IEEE Symposium on Computer Arithmetic, 1978. | recalled | Carry-free addition in base 2 on {−1, 0, 1} |
| FPS11 | C. Frougny, E. Pelantová, M. Svobodová. *Parallel addition in non-standard numeration systems.* Theoretical Computer Science 412, 2011. arXiv:1102.5683. | read | When parallel addition exists; the golden-ratio algorithms and their locality |
| FPS16 | C. Frougny, E. Pelantová, M. Svobodová. *Minimal digit sets for parallel addition in non-standard numeration systems.* arXiv:1610.08299. | read | Smallest alphabets, including φ and −1+i |
| TaizhanovSeilkhan26 | A. Taizhanov, M. Seilkhan. *Intrinsic redundancy and local robustness in finite β-expansion systems.* arXiv:2607.08795, 2026. | abstract | What β-redundancy can and cannot repair |
| Penney65 | W. Penney. *A "binary" system for complex numbers.* Journal of the ACM 12(2), 1965. | recalled | Base −1+i |
| Garner59 | H. Garner. *The residue number system.* IRE Transactions on Electronic Computers EC-8, 1959. | recalled | Carry-free addition and multiplication; hard comparison |
| ELM08 | J. Coleman, C. Softley, J. Kadlec, R. Matousek, M. Tichy, Z. Pohl, A. Hermanek, N. Benschop. *The European Logarithmic Microprocessor.* IEEE Transactions on Computers 57(4), 2008. | recalled | A fabricated logarithmic-number processor |
| LNSMadam22 | J. Zhao, S. Dai, R. Venkatesan, B. Zimmer, M. Ali, M.-Y. Liu, B. Khailany, W. Dally, A. Anandkumar. *LNS-Madam: low-precision training in logarithmic number system using multiplicative weight update.* IEEE Transactions on Computers, 2022. arXiv:2106.13914. | abstract | Energy of logarithmic arithmetic for neural-network training |
| Posit17 | J. Gustafson, I. Yonemoto. *Beating floating point at its own game: posit arithmetic.* Supercomputing Frontiers and Innovations 4(2), 2017. | recalled | Posits |
| PositCritique19 | F. de Dinechin, L. Forget, J.-M. Muller, Y. Uguen. *Posits: the good, the bad and the ugly.* CoNGA 2019. | recalled | Hardware cost and numerical caveats of posits |
| GilesSelinger13 | B. Giles, P. Selinger. *Exact synthesis of multiqubit Clifford+T circuits.* Physical Review A 87, 032332, 2013. arXiv:1212.0506. | abstract | Exact arithmetic in the ring Z[1/√2, i] used in production |
| Shewchuk97 | J. Shewchuk. *Adaptive precision floating-point arithmetic and fast robust geometric predicates.* Discrete & Computational Geometry 18, 1997. | recalled | Exact predicates without exact numbers |
| Richardson68 | D. Richardson. *Some undecidable problems involving elementary functions of a real variable.* Journal of Symbolic Logic 33(4), 1968. | recalled | Zero-testing is undecidable for rich enough expressions |
| Lindemann82 | F. Lindemann. *Über die Zahl π.* Mathematische Annalen 20, 1882. | recalled | π is transcendental |

## Equality saturation

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| Egg21 | M. Willsey, C. Nandi, Y. R. Wang, O. Flatt, Z. Tatlock, P. Panchekha. *egg: fast and extensible equality saturation.* POPL 2021. | recalled | E-graphs as a rewrite engine |
| Egglog23 | Y. Zhang, Y. R. Wang, O. Flatt, D. Cao, P. Zucker, E. Rosenthal, Z. Tatlock, M. Willsey. *Better together: unifying Datalog and equality saturation.* PLDI 2023. | recalled | E-graphs with relational analysis |
| ROVER24 | S. Coward, T. Drane, G. Constantinides. *ROVER: RTL optimization via verified e-graph rewriting.* arXiv:2406.12421, 2024. | abstract | Up to 63% less datapath area, with a checkable certificate |

## Fabrication access

| Key | Source | Checked | Used for |
| --- | --- | --- | --- |
| TinyTapeout | *Tiny Tapeout.* <https://tinytapeout.com>, shuttle pages and FAQ. | abstract | Tile size and capacity of the cheapest route to silicon |

## Found but not read

These turned up in the literature search and look relevant. Nothing in the
notes depends on them. Only the identifier was recorded, so the first column
gives the topic, not the title.

| Identifier | Topic | Why it may matter |
| --- | --- | --- |
| arXiv:1702.06092 | Parallel needed reduction for pure interaction nets | An evaluation order that avoids useless work in parallel |
| arXiv:1609.03644 | Token-passing optimal reduction (Salikhmetov) | Optimal reduction by tokens instead of rewriting |
| arXiv:1609.03640 | Kahn process networks as interaction nets | A direct bridge between the two deterministic models |
| arXiv:0906.0380 | Symmetric interaction combinators (Mazza) | Semantics a mechanized model could build on |
| arXiv:2410.00540, arXiv:1003.4562 | Nested pattern matching in interaction nets | Richer rules than one agent pair at a time |
| arXiv:2511.15323 | SkyEgg: hardware synthesis by equality saturation | Joint scheduling and implementation selection |
| arXiv:2511.09447 | SpaDA: a spatial dataflow language for a wafer-scale engine | A programming model for a very large mesh |
| arXiv:2310.18166 | Fractional uniqueness | Uniqueness and linearity in one graded system |
| arXiv:2410.02418 | Gaussian and Eisenstein integer digit sets | Complex bases beyond −1+i |
| arXiv:2208.14610 | The Sparse Abstract Machine | Dataflow abstractions for sparse tensor algebra |
| arXiv:2312.16300 | Calyx with static and dynamic control | Mixing statically timed and handshaking hardware |
