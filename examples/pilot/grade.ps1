<#
.SYNOPSIS
Grades one solution of the model-writing pilot, or every reference and
control of the pilot.

.DESCRIPTION
The grader of docs/roadmap/v0.0.2/m2/pilot/registration.md, section
"Grading". For one solution:

  pwsh -NoProfile -File examples/pilot/grade.ps1 <task> <arm> <file>

applies the translation of the noEndlet arm (and only of that arm), places
the task's project file, its hidden test module, and the solution as
src/pilot/<task>.vib in a fresh temporary directory, runs
`vibra --format json --workspace <dir> check` and then `test`, and prints
one line: task, arm, grade (passed, refuted, unknown, or unsupported), the
closed count of a noEndlet run, each command's result, the diagnostic
codes, the tests that did not pass, and the reason of a grade that the
commands' statuses do not give: a noEndlet run that closes an ordinary let,
a command stopped at the grader's time limit, or a command that ended
without a JSON document (its result shows the exit code).

  pwsh -NoProfile -File examples/pilot/grade.ps1 -All

grades every reference in both arms, every control in both arms, the
translation of each first-arm reference, and the robustness subjects, and
exits 0 only when each one has its registered grade and evidence.

  pwsh -NoProfile -File examples/pilot/grade.ps1 -CheckReader

runs the translation's reader over every .vib file of the conformance
corpus, the standard library, and the examples, and compares it with the
corpus expectations; it calls no vibra.

  pwsh -NoProfile -File examples/pilot/grade.ps1 -VerifyRuns <runs.tsv>

checks the run records of step 40 (the runs/ directory beside runs.tsv)
against the registration: the schedule, one line and one record per
scheduled run in order, every record with its prompt digest, its calls and
their whole output, its completion, extraction, and grade, the retry and
stop rules, the setup calls, and a regrade of the 20 runs the registered
seed picks. It prints `schedule complete: N of N` and exits 0 only when
every check holds.

  pwsh -NoProfile -File examples/pilot/grade.ps1 -TranslationCounts <runs.tsv>

runs only the translation's reader, and no vibra, over the stored
solution.vib of every noEndlet run of the run table (the runs/ directory
beside it) and prints, per model and overall, the solutions read, those
the translation changed, those it left unchanged, and the readings that
failed, with the run of each failed reading. It also checks that each stored solution has the digest its
record names and that each closed count equals the run table's, and exits
0 only when both hold. It grades nothing.

The script reads no network. It writes only under a fresh directory of the
system's temporary directory, which it removes.

.PARAMETER Vibra
The path of the vibra binary. The default is target/debug/vibra(.exe) of
the repository that holds this script.

.PARAMETER TimeLimit
The wall-clock seconds each of check and test may run. The registered
limit, and the default, is 120. A command still running then is stopped
with every process it started, and the grade is unknown.
#>
[CmdletBinding(DefaultParameterSetName = 'One')]
param(
  [Parameter(ParameterSetName = 'One', Position = 0, Mandatory)]
  [string]$Task,
  [Parameter(ParameterSetName = 'One', Position = 1, Mandatory)]
  [ValidateSet('endlet', 'noEndlet')]
  [string]$Arm,
  [Parameter(ParameterSetName = 'One', Position = 2, Mandatory)]
  [string]$File,
  [Parameter(ParameterSetName = 'All', Mandatory)]
  [switch]$All,
  [Parameter(ParameterSetName = 'Reader', Mandatory)]
  [switch]$CheckReader,
  [Parameter(ParameterSetName = 'Verify', Mandatory)]
  [string]$VerifyRuns,
  [Parameter(ParameterSetName = 'Counts', Mandatory)]
  [string]$TranslationCounts,
  [Parameter(ParameterSetName = 'One')]
  [Parameter(ParameterSetName = 'All')]
  [Parameter(ParameterSetName = 'Verify')]
  [string]$Vibra = '',
  [Parameter(ParameterSetName = 'One')]
  [Parameter(ParameterSetName = 'All')]
  [ValidateRange(1, 86400)]
  [int]$TimeLimit = 120
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Pilot = $PSScriptRoot
$Repository = [IO.Path]::GetFullPath((Join-Path $Pilot '../..'))
$Tasks = 'packet', 'pair', 'positive', 'range', 'order', 'gate', 'reader', 'plan', 'optional', 'rows'
$Utf8 = [Text.UTF8Encoding]::new($false)
$script:ExitCode = 1
$TimeReason = "the grader's time limit"
$NoDocumentReason = 'the toolchain gave no document'

# The translation of the noEndlet arm (registration, "Grading", step 1). The
# reader is the grammar of docs/spec/01-source-language.md, "Grammar", with
# `let` and `module-let` taking an optional closer:
#
#   let        = "let", pattern, [ ":", type-expr ], "be", expr,
#                ( "else", body, "endlet" | [ "endlet" ] ) ;
#   module-let = "let", identifier, ":", type-expr, { visibility | doc },
#                "be", expr, [ "endlet" ] ;
#
# with the lexical rules of that chapter's "Lexical structure" and the
# section rules of its notes (any order after the name, each section at
# most once, only the sections a kind admits, every `use` first). Any
# syntax error leaves the file unchanged with a closed count of zero.
$ReaderSource = @'
using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Text;
using System.Threading;

namespace VibraPilot
{
    /// <summary>The outcome of reading one solution.</summary>
    public sealed class Translation
    {
        /// <summary>The whole file was read without a syntax error.</summary>
        public bool Read;
        /// <summary>Why the file was not read, when it was not.</summary>
        public string Problem = "";
        /// <summary>The reader ran out of stack, which is a reading that failed.</summary>
        public bool Limit;
        /// <summary>The ordinary let forms read.</summary>
        public int Ordinary;
        /// <summary>The ordinary let forms read closed by endlet.</summary>
        public int Closed;
        /// <summary>The bytes written back; the input when not read.</summary>
        public byte[] Output = new byte[0];
        /// <summary>Whether the output differs from the input.</summary>
        public bool Changed;
    }

    enum K { Ident, Word, Atom, Int, Float, Str, Char, Doc, LParen, RParen, LBrace, RBrace, LBracket, RBracket, Colon, Dot, Discard, End }

    struct Tok
    {
        public K Kind;
        public string Text;
        public int Start;
        public int End;
    }

    sealed class ReadError : Exception
    {
        public ReadError(string message) : base(message) { }
    }

    /// <summary>The reader of the noEndlet translation.</summary>
    public static class Reader
    {
        /// <summary>Inserts " endlet" after the expression of every ordinary let read without one.</summary>
        public static Translation Translate(byte[] input) { return Run(input, Mode.Translate, false); }

        /// <summary>The same, reading a module of the embedded standard library (a test of the reader).</summary>
        public static Translation Translate(byte[] input, bool library) { return Run(input, Mode.Translate, library); }

        /// <summary>Removes the closer of every ordinary let read with one (a test of the reader).</summary>
        public static Translation Strip(byte[] input, bool library) { return Run(input, Mode.Strip, library); }

        /// <summary>The token texts of a file read without error, joined by one space, or null.</summary>
        public static string Tokens(byte[] input, bool library)
        {
            Translation t = Run(input, Mode.Tokens, library);
            return t.Read ? Encoding.UTF8.GetString(t.Output) : null;
        }

        enum Mode { Translate, Strip, Tokens }

        static Translation Run(byte[] input, Mode mode, bool library)
        {
            Translation result = null;
            Exception failure = null;
            Thread thread = new Thread(() =>
            {
                try { result = Work(input, mode, library); }
                catch (Exception e) { failure = e; }
            }, 256 * 1024 * 1024);
            thread.Start();
            thread.Join();
            if (failure != null) throw new InvalidOperationException("the reader failed", failure);
            return result;
        }

        static Translation Work(byte[] input, Mode mode, bool library)
        {
            Translation t = new Translation { Output = input };
            if (input.Length >= 3 && input[0] == 0xEF && input[1] == 0xBB && input[2] == 0xBF)
            {
                t.Problem = "a byte-order mark";
                return t;
            }
            string text;
            try { text = new UTF8Encoding(false, true).GetString(input); }
            catch (DecoderFallbackException) { t.Problem = "invalid UTF-8"; return t; }
            if (text.IndexOf('\0') >= 0) { t.Problem = "a NUL"; return t; }
            try
            {
                List<Tok> tokens = Lexer.Lex(text);
                Parser parser = new Parser(tokens, library);
                parser.Module();
                t.Read = true;
                t.Ordinary = parser.Ordinary;
                t.Closed = parser.Closed;
                StringBuilder edited = new StringBuilder(text);
                if (mode == Mode.Translate)
                {
                    for (int i = parser.Open.Count - 1; i >= 0; i--) edited.Insert(parser.Open[i], " endlet");
                }
                else if (mode == Mode.Strip)
                {
                    for (int i = parser.Closers.Count - 1; i >= 0; i--)
                    {
                        int[] c = parser.Closers[i];
                        int from = c[1];
                        if (text.Substring(c[0], c[1] - c[0]).Trim().Length == 0) from = c[0];
                        else while (from > 0 && (text[from - 1] == ' ' || text[from - 1] == '\t')) from--;
                        edited.Remove(from, c[2] - from);
                    }
                }
                else
                {
                    List<string> parts = new List<string>();
                    foreach (Tok k in tokens) if (k.Kind != K.End) parts.Add(k.Text);
                    edited = new StringBuilder(string.Join(" ", parts));
                }
                t.Output = new UTF8Encoding(false).GetBytes(edited.ToString());
                t.Changed = !Same(t.Output, input);
                return t;
            }
            catch (ReadError e)
            {
                t.Problem = e.Message;
                return t;
            }
            catch (InsufficientExecutionStackException)
            {
                t.Problem = "the reader ran out of stack";
                t.Limit = true;
                return t;
            }
        }

        static bool Same(byte[] a, byte[] b)
        {
            if (a.Length != b.Length) return false;
            for (int i = 0; i < a.Length; i++) if (a[i] != b[i]) return false;
            return true;
        }
    }

    static class Lexer
    {
        static readonly HashSet<string> Reserved = new HashSet<string>(
            ("as await be case do doc else endfn enddo endif endimpl endinterface endlet " +
             "endmatch endproc endtest endtype enduse ensures external fn given if impl " +
             "interface labelled let match native of performs proc representation " +
             "requires return returns role symbol takes test then try type use visibility " +
             "void where").Split(' '));

        static readonly HashSet<string> IntegerSuffixes = new HashSet<string>(
            "i8 i16 i32 i64 u8 u16 u32 u64".Split(' '));

        static bool Letter(char c) { return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z'); }
        static bool Digit(char c) { return c >= '0' && c <= '9'; }
        static bool Hex(char c) { return Digit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'); }

        static bool WordClass(K k)
        {
            return k == K.Ident || k == K.Word || k == K.Int || k == K.Float || k == K.Atom
                || k == K.Str || k == K.Char || k == K.Doc;
        }

        public static List<Tok> Lex(string s)
        {
            List<Tok> list = new List<Tok>();
            int i = 0;
            int n = s.Length;
            while (true)
            {
                while (i < n)
                {
                    char w = s[i];
                    if (w == ' ' || w == '\t' || w == '\r' || w == '\n') { i++; continue; }
                    if (w == '/' && i + 1 < n && s[i + 1] == '/')
                    {
                        while (i < n && s[i] != '\n') i++;
                        continue;
                    }
                    break;
                }
                if (i >= n)
                {
                    list.Add(new Tok { Kind = K.End, Text = "", Start = n, End = n });
                    return list;
                }
                int start = i;
                char c = s[i];
                K kind;
                if (Letter(c) || c == '_')
                {
                    while (i < n && (Letter(s[i]) || Digit(s[i]) || s[i] == '_')) i++;
                    string word = s.Substring(start, i - start);
                    if (word == "_") kind = K.Discard;
                    else if (word.IndexOf('_') >= 0) throw new ReadError("an invalid name at " + start);
                    else kind = Reserved.Contains(word) ? K.Word : K.Ident;
                }
                else if (Digit(c) || (c == '-' && i + 1 < n && Digit(s[i + 1])))
                {
                    kind = Number(s, ref i);
                }
                else if (c == '"')
                {
                    if (i + 2 < n && s[i + 1] == '"' && s[i + 2] == '"')
                    {
                        int close = s.IndexOf("\"\"\"", i + 3, StringComparison.Ordinal);
                        if (close < 0) throw new ReadError("an unterminated doc string at " + start);
                        i = close + 3;
                        kind = K.Doc;
                    }
                    else
                    {
                        i = StringEnd(s, i);
                        kind = K.Str;
                    }
                }
                else if (c == '\'')
                {
                    i = CharacterEnd(s, i);
                    kind = K.Char;
                }
                else if (c == '@')
                {
                    if (!(i + 1 < n && (Letter(s[i + 1]) || Digit(s[i + 1]) || s[i + 1] == '_')))
                        throw new ReadError("an @ that starts no token at " + start);
                    i++;
                    while (i < n && (Letter(s[i]) || Digit(s[i]) || s[i] == '_' || s[i] == '.')) i++;
                    foreach (string part in s.Substring(start + 1, i - start - 1).Split('.'))
                    {
                        if (part.Length == 0 || !Letter(part[0])) throw new ReadError("an invalid atom at " + start);
                        foreach (char p in part) if (!Letter(p) && !Digit(p)) throw new ReadError("an invalid atom at " + start);
                    }
                    kind = K.Atom;
                }
                else
                {
                    i++;
                    switch (c)
                    {
                        case '(': kind = K.LParen; break;
                        case ')': kind = K.RParen; break;
                        case '{': kind = K.LBrace; break;
                        case '}': kind = K.RBrace; break;
                        case '[': kind = K.LBracket; break;
                        case ']': kind = K.RBracket; break;
                        case ':': kind = K.Colon; break;
                        case '.': kind = K.Dot; break;
                        default: throw new ReadError("a character that starts no token at " + start);
                    }
                }
                Tok token = new Tok { Kind = kind, Text = s.Substring(start, i - start), Start = start, End = i };
                if (list.Count > 0)
                {
                    Tok before = list[list.Count - 1];
                    if (WordClass(before.Kind) && WordClass(kind) && before.End == start)
                        throw new ReadError("a missing separator at " + start);
                }
                list.Add(token);
            }
        }

        static K Number(string s, ref int i)
        {
            int n = s.Length;
            int start = i;
            if (s[i] == '-') i++;
            while (i < n && Digit(s[i])) i++;
            bool real = false;
            if (i + 1 < n && s[i] == '.' && Digit(s[i + 1]))
            {
                real = true;
                i++;
                while (i < n && Digit(s[i])) i++;
            }
            if (i < n && (s[i] == 'e' || s[i] == 'E'))
            {
                int j = i + 1;
                if (j < n && (s[j] == '+' || s[j] == '-')) j++;
                if (j < n && Digit(s[j]))
                {
                    real = true;
                    i = j;
                    while (i < n && Digit(s[i])) i++;
                }
            }
            int suffix = i;
            while (i < n && (Letter(s[i]) || Digit(s[i]) || s[i] == '_')) i++;
            string text = s.Substring(suffix, i - suffix);
            if (text == "f32" || text == "f64") return K.Float;
            if (text.Length == 0) return real ? K.Float : K.Int;
            if (!real && IntegerSuffixes.Contains(text)) return K.Int;
            throw new ReadError("an invalid numeric literal at " + start);
        }

        static int Escape(string s, int i, bool character)
        {
            int n = s.Length;
            if (i + 1 >= n) throw new ReadError("an unterminated literal at " + i);
            char e = s[i + 1];
            if (e == '\\' || e == '"' || e == 'n' || e == 'r' || e == 't') return i + 2;
            if (character && e == '\'') return i + 2;
            if (e == 'u')
            {
                int j = i + 2;
                if (j >= n || s[j] != '{') throw new ReadError("an invalid escape at " + i);
                j++;
                int digits = 0;
                long value = 0;
                while (j < n && Hex(s[j]))
                {
                    value = value * 16 + Convert.ToInt32(s[j].ToString(), 16);
                    digits++;
                    j++;
                    if (digits > 6) throw new ReadError("an invalid escape at " + i);
                }
                if (digits == 0 || j >= n || s[j] != '}') throw new ReadError("an invalid escape at " + i);
                if (value > 0x10FFFF || (value >= 0xD800 && value <= 0xDFFF)) throw new ReadError("an invalid escape at " + i);
                return j + 1;
            }
            throw new ReadError("an invalid escape at " + i);
        }

        static int StringEnd(string s, int i)
        {
            int start = i;
            i++;
            while (true)
            {
                if (i >= s.Length) throw new ReadError("an unterminated string at " + start);
                char c = s[i];
                if (c == '"') return i + 1;
                if (c == '\\') i = Escape(s, i, false);
                else i++;
            }
        }

        static int CharacterEnd(string s, int i)
        {
            int start = i;
            int n = s.Length;
            i++;
            if (i >= n || s[i] == '\'' || s[i] == '\n' || s[i] == '\r') throw new ReadError("an invalid character literal at " + start);
            if (s[i] == '\\') i = Escape(s, i, true);
            else if (char.IsHighSurrogate(s[i]) && i + 1 < n && char.IsLowSurrogate(s[i + 1])) i += 2;
            else i++;
            if (i >= n || s[i] != '\'') throw new ReadError("an invalid character literal at " + start);
            return i + 1;
        }
    }

    sealed class Parser
    {
        static readonly HashSet<string> Sections = new HashSet<string>(
            "takes labelled returns given requires ensures performs visibility external symbol native doc where role representation".Split(' '));
        static readonly HashSet<string> FnSections = new HashSet<string>(
            "takes labelled returns given requires ensures visibility external symbol native doc".Split(' '));
        static readonly HashSet<string> ProcSections = new HashSet<string>(
            "takes labelled returns given requires ensures performs visibility external symbol doc".Split(' '));
        static readonly HashSet<string> AnonymousFnSections = new HashSet<string>(
            "takes labelled returns given requires ensures".Split(' '));
        static readonly HashSet<string> AnonymousProcSections = new HashSet<string>(
            "takes labelled returns given requires ensures performs".Split(' '));
        // Admitted only in the embedded standard library (01 "External definitions").
        static readonly HashSet<string> LibrarySections = new HashSet<string>(
            "role external symbol native".Split(' '));
        // The closed tag set of each section whose operand is a tag.
        static readonly Dictionary<string, HashSet<string>> Tags = new Dictionary<string, HashSet<string>>
        {
            { "visibility", new HashSet<string>("@public @private".Split(' ')) },
            { "representation", new HashSet<string>("@public @private".Split(' ')) },
            { "external", new HashSet<string>("@compiler @host".Split(' ')) },
            { "role", new HashSet<string>("@bool @str @bytes @option @result @dict @iter @equatable @ordered @encodable".Split(' ')) },
        };
        // The sections a member does not declare: of an impl, the guards and
        // visibility; of an interface, visibility.
        static readonly HashSet<string> ImplMemberExcluded = new HashSet<string>("requires ensures where visibility".Split(' '));
        static readonly HashSet<string> InterfaceMemberExcluded = new HashSet<string>("visibility".Split(' '));

        readonly List<Tok> t;
        readonly bool library;
        int p;

        /// <summary>Ordinary let forms read.</summary>
        public int Ordinary;
        /// <summary>Ordinary let forms read closed by endlet.</summary>
        public int Closed;
        /// <summary>Where " endlet" goes for each ordinary let read without one.</summary>
        public readonly List<int> Open = new List<int>();
        /// <summary>For each closed ordinary let: the end of its expression, and the closer's span.</summary>
        public readonly List<int[]> Closers = new List<int[]>();

        public Parser(List<Tok> tokens, bool library) { t = tokens; this.library = library; }

        Tok Peek { get { return t[p]; } }
        Tok Ahead(int k) { return t[Math.Min(p + k, t.Count - 1)]; }
        int LastEnd { get { return p > 0 ? t[p - 1].End : 0; } }
        bool Is(string word) { return t[p].Kind == K.Word && t[p].Text == word; }
        bool Is(K kind) { return t[p].Kind == kind; }
        // Directly in a bracket group a reserved word before `:` is a
        // recovered label or binder (01 "Reserved words").
        bool Labelled() { return (t[p].Kind == K.Ident || t[p].Kind == K.Word) && Ahead(1).Kind == K.Colon; }

        // In a name position a reserved word other than the construct's own
        // closer is taken as the name: the reader reports it as a name
        // diagnostic, not a syntax error.
        void Recover(string closer)
        {
            if (t[p].Kind == K.Word && t[p].Text != closer)
            {
                Tok word = t[p];
                word.Kind = K.Ident;
                t[p] = word;
            }
        }

        void RecoverLabel() { if (t[p].Kind == K.Word && Ahead(1).Kind == K.Colon) Recover(""); }

        // The openers and closers of constructs; any other reserved word in
        // a pattern is a leaf, which the reader recovers as a binder.
        static readonly HashSet<string> Delimiters = new HashSet<string>(
            ("type endtype interface endinterface impl endimpl fn endfn proc endproc let endlet " +
             "test endtest use enduse if endif match endmatch do enddo").Split(' '));

        void Tag(string section)
        {
            if (!Is(K.Atom) || !Tags[section].Contains(Peek.Text)) throw Error("a tag outside the set of " + section);
            p++;
        }

        void Admit(string section)
        {
            if (!library && LibrarySections.Contains(section)) throw Error("a section only the standard library admits: " + section);
        }

        ReadError Error(string what) { return new ReadError(what + " at " + t[p].Start); }
        void Expect(string word) { if (!Is(word)) throw Error("expected " + word); p++; }
        void Expect(K kind) { if (!Is(kind)) throw Error("expected " + kind); p++; }
        void Name() { Expect(K.Ident); }
        static void Guard() { RuntimeHelpers.EnsureSufficientExecutionStack(); }
        void Once(HashSet<string> seen, string section) { if (!seen.Add(section)) throw Error("a repeated section " + section); }

        public void Module()
        {
            bool other = false;
            while (!Is(K.End))
            {
                if (Is("use"))
                {
                    if (other) throw Error("a use after another form");
                    p++;
                    Expect(K.Atom);
                    if (Is("as")) { p++; Recover("enduse"); Name(); }
                    Expect("enduse");
                    continue;
                }
                other = true;
                if (Is("type")) TypeDeclaration();
                else if (Is("interface")) Interface();
                else if (Is("fn")) Callable("fn", true, "");
                else if (Is("proc")) Callable("proc", true, "");
                else if (Is("let")) ModuleLet();
                else if (Is("test")) Test();
                else throw Error("not a top-level form");
            }
        }

        void TypeDeclaration()
        {
            Guard();
            Expect("type");
            Recover("endtype");
            Name();
            if (Is(K.LParen)) TypeParameters();
            HashSet<string> seen = new HashSet<string>();
            bool of = false;
            bool cases = false;
            while (!Is("endtype"))
            {
                if (!Is(K.Word)) throw Error("not a type item");
                switch (Peek.Text)
                {
                    case "of":
                        if (of || cases) throw Error("a second representation");
                        of = true;
                        p++;
                        if (Is(K.Ident) && Peek.Text == "Intrinsic")
                        {
                            if (!library) throw Error("Intrinsic outside the standard library");
                            p++;
                            Expect(K.LParen);
                            Expect(K.Atom);
                            Expect(K.RParen);
                        }
                        else TypeExpression();
                        break;
                    case "case":
                        if (of) throw Error("a case after of");
                        cases = true;
                        p++;
                        Name();
                        if (Is(K.LBrace)) RecordType();
                        break;
                    case "given": Once(seen, "given"); p++; Given(); break;
                    case "where": Once(seen, "where"); p++; Guarded(); break;
                    case "role":
                    case "visibility":
                    case "representation":
                        {
                            string section = Peek.Text;
                            Admit(section);
                            Once(seen, section);
                            p++;
                            Tag(section);
                            break;
                        }
                    case "doc": Once(seen, "doc"); p++; Expect(K.Doc); break;
                    case "fn": Callable("fn", true, ""); break;
                    case "proc": Callable("proc", true, ""); break;
                    case "impl": Impl(); break;
                    default: throw Error("not a type item");
                }
            }
            if (!of && !cases) throw Error("a type with no representation");
            p++;
        }

        void Interface()
        {
            Guard();
            Expect("interface");
            Recover("endinterface");
            Name();
            if (Is(K.LParen)) TypeParameters();
            HashSet<string> seen = new HashSet<string>();
            while (!Is("endinterface"))
            {
                if (!Is(K.Word)) throw Error("not an interface item");
                switch (Peek.Text)
                {
                    case "given": Once(seen, "given"); p++; Given(); break;
                    case "role":
                    case "visibility":
                        {
                            string section = Peek.Text;
                            Admit(section);
                            Once(seen, section);
                            p++;
                            Tag(section);
                            break;
                        }
                    case "doc": Once(seen, "doc"); p++; Expect(K.Doc); break;
                    case "type": p++; Recover("endtype"); Name(); Expect("endtype"); break;
                    case "fn": Callable("fn", true, "interface"); break;
                    case "proc": Callable("proc", true, "interface"); break;
                    case "impl": Impl(); break;
                    default: throw Error("not an interface item");
                }
            }
            p++;
        }

        void Impl()
        {
            Guard();
            Expect("impl");
            TypeExpression();
            while (!Is("endimpl"))
            {
                if (Is("type")) { p++; Recover("endtype"); Name(); Expect("of"); TypeExpression(); Expect("endtype"); }
                else if (Is("fn")) Callable("fn", true, "impl");
                else if (Is("proc")) Callable("proc", true, "impl");
                else throw Error("not an impl item");
            }
            p++;
        }

        void Callable(string keyword, bool named, string member)
        {
            Guard();
            Expect(keyword);
            if (named) Name();
            if (Is(K.LParen)) TypeParameters();
            HashSet<string> allowed = named
                ? (keyword == "fn" ? FnSections : ProcSections)
                : (keyword == "fn" ? AnonymousFnSections : AnonymousProcSections);
            HashSet<string> seen = new HashSet<string>();
            while (Is(K.Word) && Sections.Contains(Peek.Text))
            {
                string section = Peek.Text;
                if (!allowed.Contains(section)) throw Error("a section the form does not admit: " + section);
                if (member == "impl" && ImplMemberExcluded.Contains(section)) throw Error("a section an impl member does not declare: " + section);
                if (member == "interface" && InterfaceMemberExcluded.Contains(section)) throw Error("a section an interface member does not declare: " + section);
                Admit(section);
                Once(seen, section);
                p++;
                switch (section)
                {
                    case "takes": Parameters(); break;
                    case "labelled": LabelledParameters(); break;
                    case "returns": Recover(keyword == "fn" ? "endfn" : "endproc"); Name(); Expect(K.Colon); TypeExpression(); break;
                    case "given": Given(); break;
                    case "requires":
                    case "ensures": Guarded(); break;
                    case "performs": Row(); break;
                    case "visibility":
                    case "external": Tag(section); break;
                    case "symbol":
                    case "native": Expect(K.Str); break;
                    case "doc": Expect(K.Doc); break;
                }
            }
            if (!seen.Contains("returns")) throw Error("a callable with no returns");
            if (Is("do")) { p++; Body(); }
            else if (!named || (member != "interface" && !seen.Contains("external")))
                throw Error("a callable with no body that is neither abstract nor external");
            Expect(keyword == "fn" ? "endfn" : "endproc");
        }

        void Test()
        {
            Guard();
            Expect("test");
            Expect(K.Str);
            HashSet<string> seen = new HashSet<string>();
            while (Is(K.Word) && Sections.Contains(Peek.Text))
            {
                string section = Peek.Text;
                if (section != "takes" && section != "performs") throw Error("a section a test does not admit: " + section);
                Once(seen, section);
                p++;
                if (section == "performs") { Row(); continue; }
                // One bare-name parameter, or none (01 "test").
                Expect(K.LBrace);
                if (Is(K.RBrace)) { p++; continue; }
                RecoverLabel();
                Name();
                Expect(K.Colon);
                TypeExpression();
                Expect(K.RBrace);
            }
            Expect("do");
            Body();
            Expect("endtest");
        }

        void ModuleLet()
        {
            Guard();
            Expect("let");
            Recover("endlet");
            Name();
            Expect(K.Colon);
            TypeExpression();
            HashSet<string> seen = new HashSet<string>();
            while (Is("visibility") || Is("doc"))
            {
                Once(seen, Peek.Text);
                bool visibility = Is("visibility");
                p++;
                if (visibility) Tag("visibility"); else Expect(K.Doc);
            }
            Expect("be");
            Expression();
            CloseOrdinary();
        }

        void Let()
        {
            Guard();
            Expect("let");
            Pattern("endlet");
            if (Is(K.Colon)) { p++; TypeExpression(); }
            Expect("be");
            Expression();
            if (Is("else"))
            {
                p++;
                Body();
                Expect("endlet");
            }
            else CloseOrdinary();
        }

        void CloseOrdinary()
        {
            Ordinary++;
            if (Is("endlet"))
            {
                Closed++;
                Closers.Add(new int[] { LastEnd, Peek.Start, Peek.End });
                p++;
            }
            else Open.Add(LastEnd);
        }

        void Guarded()
        {
            Expression();
            if (Is("else")) { p++; Expression(); }
        }

        bool StartsStatement()
        {
            switch (Peek.Kind)
            {
                case K.Ident:
                case K.Atom:
                case K.Int:
                case K.Float:
                case K.Str:
                case K.Char:
                case K.LBrace:
                case K.LBracket:
                    return true;
                case K.Word:
                    switch (Peek.Text)
                    {
                        case "let":
                        case "return":
                        case "try":
                        case "await":
                        case "if":
                        case "match":
                        case "do":
                        case "fn":
                        case "proc":
                        case "void":
                            return true;
                    }
                    return false;
                default:
                    return false;
            }
        }

        void Body()
        {
            Guard();
            while (StartsStatement())
            {
                if (Is("let")) Let(); else Expression();
            }
        }

        void Expression()
        {
            Guard();
            if (Is("return")) { p++; Expression(); return; }
            while (Is("try") || Is("await")) p++;
            Primary();
            while (true)
            {
                if (Is(K.LParen)) { p++; Arguments(); }
                else if (Is(K.Dot)) { p++; Name(); }
                else return;
            }
        }

        void Arguments()
        {
            while (!Is(K.RParen))
            {
                if (Labelled()) p += 2;
                Expression();
            }
            p++;
        }

        void Primary()
        {
            Guard();
            switch (Peek.Kind)
            {
                case K.Ident:
                case K.Atom:
                case K.Int:
                case K.Float:
                case K.Str:
                case K.Char:
                    p++;
                    return;
                case K.LBrace:
                    p++;
                    if (Is(K.RBrace)) { p++; return; }
                    if (Labelled())
                    {
                        while (!Is(K.RBrace))
                        {
                            if (!Labelled()) throw Error("a record field with no name");
                            p += 2;
                            Expression();
                        }
                    }
                    else
                    {
                        while (!Is(K.RBrace))
                        {
                            Expression();
                            if (Is(K.Colon)) throw Error("a tuple with a field");
                        }
                    }
                    p++;
                    return;
                case K.LBracket:
                    p++;
                    if (Is(K.RBracket)) { p++; return; }
                    if (Is(K.Colon) && Ahead(1).Kind == K.RBracket) { p += 2; return; }
                    Expression();
                    if (Is(K.Colon))
                    {
                        p++;
                        Expression();
                        while (!Is(K.RBracket))
                        {
                            Expression();
                            Expect(K.Colon);
                            Expression();
                        }
                    }
                    else
                    {
                        while (!Is(K.RBracket))
                        {
                            Expression();
                            if (Is(K.Colon)) throw Error("a list with a key");
                        }
                    }
                    p++;
                    return;
                case K.Word:
                    switch (Peek.Text)
                    {
                        case "void":
                            p++;
                            return;
                        case "if":
                            p++;
                            Expression();
                            Expect("then");
                            Body();
                            if (Is("else")) { p++; Body(); }
                            Expect("endif");
                            return;
                        case "match":
                            p++;
                            Expression();
                            if (!Is("case")) throw Error("a match with no arm");
                            while (Is("case")) { p++; Pattern("endmatch"); Body(); }
                            Expect("endmatch");
                            return;
                        case "do":
                            p++;
                            Body();
                            Expect("enddo");
                            return;
                        case "fn":
                            Callable("fn", false, "");
                            return;
                        case "proc":
                            Callable("proc", false, "");
                            return;
                    }
                    break;
            }
            throw Error("not an expression");
        }

        void Pattern() { Pattern(null); }

        // A reserved word other than `void` is a recovered one-word binder:
        // in a name position (the first word of a let or of a match arm)
        // unless it is that construct's closer, and elsewhere unless it is
        // an opener or closer, which the reader never takes as a leaf.
        void Pattern(string closer)
        {
            Guard();
            if (Is(K.Word) && Peek.Text != "void"
                && (closer != null ? Peek.Text != closer : !Delimiters.Contains(Peek.Text)))
            {
                p++;
                return;
            }
            switch (Peek.Kind)
            {
                case K.Discard:
                case K.Str:
                case K.Char:
                case K.Int:
                case K.Atom:
                    p++;
                    return;
                case K.Ident:
                    Path();
                    if (Is(K.LParen))
                    {
                        p++;
                        if (Labelled())
                        {
                            while (!Is(K.RParen))
                            {
                                if (!Labelled()) throw Error("a pattern field with no name");
                                p += 2;
                                Pattern();
                            }
                        }
                        else while (!Is(K.RParen)) Pattern();
                        p++;
                    }
                    return;
                case K.LBrace:
                    p++;
                    if (Labelled())
                    {
                        while (!Is(K.RBrace))
                        {
                            if (!Labelled()) throw Error("a pattern field with no name");
                            p += 2;
                            Pattern();
                        }
                    }
                    else
                    {
                        while (!Is(K.RBrace))
                        {
                            Pattern();
                            if (Is(K.Colon)) throw Error("a tuple pattern with a field");
                        }
                    }
                    p++;
                    return;
                case K.LBracket:
                    p++;
                    while (!Is(K.RBracket)) Pattern();
                    p++;
                    return;
            }
            throw Error("not a pattern");
        }

        void Path()
        {
            Name();
            while (Is(K.Dot)) { p++; Name(); }
        }

        void TypeExpression()
        {
            Guard();
            if (Is(K.Ident))
            {
                string head = Peek.Text;
                if (head == "Fn" || head == "Proc" || head == "Intent")
                {
                    p++;
                    Expect(K.LParen);
                    if (head != "Intent") Inputs();
                    TypeExpression();
                    if (head != "Fn" && !Is(K.RParen)) Row();
                    Expect(K.RParen);
                    return;
                }
                if (head == "Intrinsic") throw Error("Intrinsic outside a type body");
                Path();
                if (Is(K.LParen))
                {
                    p++;
                    bool labels = false;
                    while (!Is(K.RParen))
                    {
                        if (Labelled()) { labels = true; p += 2; }
                        else if (labels) throw Error("a positional type argument after a labelled one");
                        TypeExpression();
                    }
                    p++;
                }
                return;
            }
            if (Is(K.Atom)) { p++; return; }
            if (Is(K.LBrace))
            {
                if ((Ahead(1).Kind == K.Ident || Ahead(1).Kind == K.Word) && Ahead(2).Kind == K.Colon) { RecordType(); return; }
                p++;
                while (!Is(K.RBrace)) TypeExpression();
                p++;
                return;
            }
            throw Error("not a type");
        }

        void RecordType()
        {
            Expect(K.LBrace);
            if (!Labelled()) throw Error("a record type with no field");
            while (!Is(K.RBrace))
            {
                if (!Labelled()) throw Error("a record type field with no name");
                p += 2;
                TypeExpression();
            }
            p++;
        }

        void Inputs()
        {
            Expect(K.LBrace);
            bool labels = false;
            while (!Is(K.RBrace))
            {
                if (Labelled()) { labels = true; p += 2; }
                else if (labels) throw Error("a positional input after a labelled one");
                TypeExpression();
            }
            p++;
        }

        void Row()
        {
            Expect(K.LBrace);
            while (!Is(K.RBrace)) Path();
            p++;
        }

        void Given()
        {
            Expect(K.LBrace);
            while (!Is(K.RBrace)) { RecoverLabel(); Name(); Expect(K.Colon); TypeExpression(); }
            p++;
        }

        void Parameters()
        {
            Expect(K.LBrace);
            while (!Is(K.RBrace)) { RecoverLabel(); Pattern(); Expect(K.Colon); TypeExpression(); }
            p++;
        }

        void LabelledParameters()
        {
            Expect(K.LBrace);
            while (!Is(K.RBrace)) { RecoverLabel(); Name(); Expect(K.Colon); TypeExpression(); Expect("be"); Expression(); }
            p++;
        }

        void TypeParameters()
        {
            Expect(K.LParen);
            Name();
            while (!Is(K.RParen)) Name();
            p++;
        }
    }
}
'@
if (-not ('VibraPilot.Reader' -as [type])) {
  Add-Type -TypeDefinition $ReaderSource -Language CSharp
}

function Get-RegistrationLines {
  $path = Join-Path $Repository 'docs/roadmap/v0.0.2/m2/pilot/registration.md'
  return [IO.File]::ReadAllLines($path)
}

function Get-Sha256([byte[]]$Bytes) {
  return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()
}

function Resolve-VibraPath {
  $path = $Vibra
  if ($path -eq '') {
    $name = if ($IsWindows) { 'vibra.exe' } else { 'vibra' }
    $path = Join-Path $Repository "target/debug/$name"
  }
  $path = [IO.Path]::GetFullPath($path, (Get-Location).Path)
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "no vibra binary at $path" }
  return $path
}

# Runs `vibra --format json --workspace <dir> <command>` and returns how it
# ended: Document is its JSON document, or $null when the command was still
# running after $Limit seconds (Stopped; it is then stopped with every
# process it started) or ended without a JSON document (Exit holds its exit
# code).
function Invoke-Vibra([string]$Directory, [string]$Command, [int]$Limit) {
  $start = [Diagnostics.ProcessStartInfo]::new($script:VibraPath)
  foreach ($argument in '--format', 'json', '--workspace', $Directory, $Command) { $start.ArgumentList.Add($argument) }
  $start.UseShellExecute = $false
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.StandardOutputEncoding = $Utf8
  $start.StandardErrorEncoding = $Utf8
  $start.WorkingDirectory = $Directory
  $process = [Diagnostics.Process]::Start($start)
  try {
    $errors = $process.StandardError.ReadToEndAsync()
    $output = $process.StandardOutput.ReadToEndAsync()
    if (-not $process.WaitForExit($Limit * 1000)) {
      $process.Kill($true)
      $process.WaitForExit()
      return [pscustomobject]@{ Document = $null; Stopped = $true; Exit = $null }
    }
    $process.WaitForExit()
    $text = $output.Result
    $null = $errors.Result
    $exit = $process.ExitCode
  }
  finally {
    if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    $process.Dispose()
  }
  $document = $null
  if (-not [string]::IsNullOrWhiteSpace($text)) {
    try { $document = $text | ConvertFrom-Json -Depth 100 } catch { $document = $null }
  }
  return [pscustomobject]@{ Document = $document; Stopped = $false; Exit = $exit }
}

# The document of a command, or for a command that has none the status
# unknown with its registered reason (registration, "Grading", step 3).
function Get-Outcome($Ended) {
  if ($null -ne $Ended.Document) { return [pscustomobject]@{ Document = $Ended.Document; Reason = ''; Result = $Ended.Document.result } }
  if ($Ended.Stopped) { $reason = $TimeReason; $result = 'stopped' }
  else { $reason = $NoDocumentReason; $result = 'noDocument(exit 0x{0:X8})' -f $Ended.Exit }
  $document = [pscustomobject]@{ status = 'unknown'; result = $result; diagnostics = @(); payload = $null }
  return [pscustomobject]@{ Document = $document; Reason = $reason; Result = $result }
}

function Get-Codes($Document) {
  $codes = [Collections.Generic.List[string]]::new()
  foreach ($diagnostic in @($Document.diagnostics)) {
    if ($null -ne $diagnostic -and -not $codes.Contains($diagnostic.code)) { $codes.Add($diagnostic.code) }
  }
  return , $codes
}

# Grades one solution (registration, "Grading", steps 1 to 4).
function Get-Grade([string]$TaskName, [string]$ArmName, [byte[]]$Solution, [int]$Limit = $TimeLimit) {
  if ($TaskName -notin $Tasks) { throw "no task $TaskName" }
  # An empty file is an empty solution, not a missing one.
  if ($null -eq $Solution) { $Solution = [byte[]]::new(0) }
  $taskDirectory = Join-Path $Pilot $TaskName
  $translation = $null
  $placed = $Solution
  if ($ArmName -eq 'noEndlet') {
    # A reading that fails anywhere, the reader's stack included, leaves the
    # file unchanged with a closed count of zero.
    $translation = [VibraPilot.Reader]::Translate($Solution)
    $placed = $translation.Output
  }
  $work = Join-Path ([IO.Path]::GetTempPath()) ('vibraPilot-' + [guid]::NewGuid().ToString('N'))
  try {
    $null = New-Item -ItemType Directory -Path $work, (Join-Path $work 'src/pilot'), (Join-Path $work 'tests')
    Copy-Item -LiteralPath (Join-Path $taskDirectory 'project.vibon') -Destination $work
    Copy-Item -LiteralPath (Join-Path $taskDirectory "tests/$TaskName.vib") -Destination (Join-Path $work 'tests')
    [IO.File]::WriteAllBytes((Join-Path $work "src/pilot/$TaskName.vib"), $placed)
    $check = Invoke-Vibra $work 'check' $Limit
    $test = Invoke-Vibra $work 'test' $Limit
  }
  finally {
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
  }
  $checkOutcome = Get-Outcome $check
  $testOutcome = Get-Outcome $test
  $check = $checkOutcome.Document
  $test = $testOutcome.Document
  $codes = Get-Codes $check
  $failed = [Collections.Generic.List[string]]::new()
  if ($check.status -eq 'passed' -and $null -ne $test.payload) {
    foreach ($code in (Get-Codes $test)) { if (-not $codes.Contains($code)) { $codes.Add($code) } }
    foreach ($item in @($test.payload.tests)) {
      foreach ($code in @($item.diagnostics)) { if ($null -ne $code -and -not $codes.Contains($code)) { $codes.Add($code) } }
      if ($item.result -ne '@test.passed') {
        $name = if ($item.name -match '::"(.*)"$') { $Matches[1] } else { $item.name }
        $failed.Add("$name ($($item.result))")
      }
    }
  }
  $closed = if ($null -ne $translation) { $translation.Closed } else { $null }
  $reason = ''
  if ($ArmName -eq 'noEndlet' -and $closed -gt 0) {
    $grade = 'refuted'
    $reason = 'an ordinary let is closed'
  }
  elseif ($check.status -ne 'passed') {
    $grade = $check.status
    $reason = $checkOutcome.Reason
  }
  else {
    $grade = $test.status
    $reason = $testOutcome.Reason
  }
  if ($grade -notin 'passed', 'refuted', 'unknown', 'unsupported') { throw "vibra reported the status '$grade'" }
  return [pscustomobject]@{
    Task = $TaskName; Arm = $ArmName; Grade = $grade; Reason = $reason
    Closed = $closed; Translation = $translation
    Check = $check.result; Test = $test.result
    Codes = $codes; Failed = $failed
  }
}

function Format-Grade($Graded) {
  $fields = @(
    $Graded.Task
    $Graded.Arm
    $Graded.Grade
    "closed=$(if ($null -ne $Graded.Closed) { $Graded.Closed } else { '-' })"
    "check=$($Graded.Check)"
    "test=$($Graded.Test)"
    "codes=$(if ($Graded.Codes.Count) { $Graded.Codes -join ',' } else { '-' })"
    "failed=$(if ($Graded.Failed.Count) { $Graded.Failed -join '; ' } else { '-' })"
  )
  if ($Graded.Reason) { $fields += "reason=$($Graded.Reason)" }
  return $fields -join "`t"
}

# What the registration fixes for the -All run: the statement digests, the
# hidden test names in order, and the controls with their expectation.
function Test-Registration {
  $lines = Get-RegistrationLines
  $problems = [Collections.Generic.List[string]]::new()
  $ids = @{}
  for ($i = 0; $i -lt $Tasks.Count; $i++) { $ids[$Tasks[$i]] = "T$($i + 1)" }
  foreach ($task in $Tasks) {
    $row = $lines | Where-Object { $_.StartsWith("| Statement of $($ids[$task]) ``$task``, both arms |") }
    $statement = [IO.File]::ReadAllBytes((Join-Path $Pilot "$task/statement.txt"))
    if (-not $row -or $row -notmatch "\| $($statement.Length) \| ``$(Get-Sha256 $statement)`` \|$") { $problems.Add("$task statement.txt is not the registered statement") }
    $heading = [Array]::IndexOf($lines, "### $($ids[$task]) ``$task``", [Array]::IndexOf($lines, '## Hidden tests'))
    $names = [Collections.Generic.List[string]]::new()
    for ($j = $heading + 1; $j -lt $lines.Count -and -not $lines[$j].StartsWith('#'); $j++) {
      if ($lines[$j] -match '^\| (.+?) \| ' -and $Matches[1] -notin 'Test', '---') { $names.Add($Matches[1]) }
    }
    $written = [regex]::Matches([IO.File]::ReadAllText((Join-Path $Pilot "$task/tests/$task.vib")), '(?m)^test "(.*)"$') | ForEach-Object { $_.Groups[1].Value }
    if (($names -join "`n") -ne (@($written) -join "`n")) { $problems.Add("$task hidden tests are not the registered tests") }
  }
  $controls = Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $Pilot 'controls.tsv')
  $section = [Array]::IndexOf($lines, '## Negative controls')
  $end = [Array]::IndexOf($lines, '## Budget')
  $registered = $lines[$section..$end] | Where-Object { $_ -match '^\| T\d+ \| `[a-zA-Z]+` \|' }
  if (@($registered).Count -ne @($controls).Count) { $problems.Add('controls.tsv does not list every registered control') }
  foreach ($control in $controls) {
    $row = $registered | Where-Object { $_ -match "^\| $($ids[$control.task]) \| ``$($control.control)`` \|" }
    $evidence = ($control.shows -split ' ', 2)[1]
    if (-not $row -or -not $row.Contains($evidence)) { $problems.Add("$($control.control) is not registered with $evidence") }
  }
  return , $problems
}

# Whether a graded subject has its expected grade and evidence.
function Test-Expected($Graded, [string]$Grade, [string]$Shows) {
  if ($Graded.Grade -ne $Grade) { return $false }
  if ($Graded.Arm -eq 'noEndlet' -and $Shows -ne 'reason closed' -and $Graded.Closed -ne 0) { return $false }
  $kind, $value = $Shows -split ' ', 2
  switch ($kind) {
    'none' { return $Graded.Codes.Count -eq 0 }
    'any' { return $true }
    'code' { return $Graded.Codes.Contains($value) }
    'test' { return [bool]($Graded.Failed | Where-Object { $_.StartsWith("$value (") }) }
    'reason' {
      if ($value -eq 'closed') { return $Graded.Reason -eq 'an ordinary let is closed' }
      if ($value -eq 'time') { return $Graded.Reason -eq $TimeReason }
      if ($value -eq 'noDocument') { return $Graded.Reason -eq $NoDocumentReason }
    }
  }
  throw "no expectation kind $kind"
}

function Invoke-All {
  $problems = Test-Registration
  foreach ($problem in $problems) { "REGISTRATION`t$problem" }
  $subjects = [Collections.Generic.List[object]]::new()
  $controls = Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $Pilot 'controls.tsv')
  foreach ($task in $Tasks) {
    foreach ($arm in 'endlet', 'noEndlet') {
      $subjects.Add(@{ Subject = 'reference'; Task = $task; Arm = $arm; Path = "$task/$arm/reference.vib"; Grade = 'passed'; Shows = 'none' })
    }
    $first = [IO.File]::ReadAllBytes((Join-Path $Pilot "$task/endlet/reference.vib"))
    $local = [VibraPilot.Reader]::Translate($first)
    $subjects.Add(@{
        Subject = 'translationIsLocal'; Task = $task; Arm = 'noEndlet'; Path = "$task/endlet/reference.vib"
        Grade = if ($local.Ordinary -gt 0) { 'refuted' } else { 'passed' }
        Shows = if ($local.Ordinary -gt 0) { 'reason closed' } else { 'none' }
        Local = $true
      })
    foreach ($control in ($controls | Where-Object { $_.task -eq $task })) {
      foreach ($arm in 'endlet', 'noEndlet') {
        $subjects.Add(@{ Subject = $control.control; Task = $task; Arm = $arm; Path = "$task/$arm/$($control.control).vib"; Grade = $control.grade; Shows = $control.shows })
      }
    }
  }
  # A row with a limit runs with that many seconds instead of -TimeLimit, so
  # that the self-check of the time limit does not take the registered 120.
  foreach ($row in (Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $Pilot 'robustness.tsv'))) {
    $subject = @{ Subject = $row.subject; Task = $row.task; Arm = $row.arm; Path = $row.file; Grade = $row.grade; Shows = $row.shows }
    if ($row.limit) { $subject.Limit = [int]$row.limit }
    $subjects.Add($subject)
  }
  foreach ($arm in 'endlet', 'noEndlet') {
    $subjects.Add(@{ Subject = 'notUtf8'; Task = 'packet'; Arm = $arm; Bytes = [byte[]](0x66, 0x6E, 0x20, 0xC3, 0x28, 0xFF, 0x0A); Grade = 'refuted'; Shows = 'any' })
  }
  # The packet reference with its stop code nested in a million list
  # brackets, deeper than the reader's stack: the reading fails, so the
  # file is placed unchanged with a closed count of zero. The toolchain
  # ends `check` on it without a document, which is unknown.
  foreach ($arm in 'endlet', 'noEndlet') {
    $reference = [IO.File]::ReadAllText((Join-Path $Pilot "packet/$arm/reference.vib"))
    $depth = 1000000
    $nested = $reference.Replace('be 99i32', 'be ' + ('[' * $depth) + '99i32' + (']' * $depth))
    $subjects.Add(@{ Subject = 'deep'; Task = 'packet'; Arm = $arm; Bytes = $Utf8.GetBytes($nested); Grade = 'unknown'; Shows = 'reason noDocument'; Deep = $true })
  }
  $met = 0
  foreach ($subject in $subjects) {
    if ($subject.ContainsKey('Bytes')) { $bytes = $subject.Bytes }
    else { $bytes = [IO.File]::ReadAllBytes((Join-Path $Pilot $subject.Path)) }
    $limit = if ($subject.ContainsKey('Limit')) { $subject.Limit } else { $TimeLimit }
    $graded = Get-Grade $subject.Task $subject.Arm $bytes $limit
    $ok = Test-Expected $graded $subject.Grade $subject.Shows
    if ($subject.ContainsKey('Deep') -and $subject.Arm -eq 'noEndlet') {
      $translation = $graded.Translation
      $ok = $ok -and $translation.Limit -and -not $translation.Read -and -not $translation.Changed
    }
    if ($subject.ContainsKey('Local')) {
      $translation = $graded.Translation
      $ok = $ok -and $translation.Read -and -not $translation.Changed -and $translation.Closed -eq $translation.Ordinary
    }
    if ($ok) { $met++ }
    $verdict = if ($ok) { 'as registered' } else { "NOT AS REGISTERED: expected $($subject.Grade), $($subject.Shows)" }
    "$($subject.Subject)`t$(Format-Grade $graded)`t$verdict"
  }
  # The authority subject is refuted only because the grader restores the
  # task's project and tests: run as the solution supplied them, they pass.
  $work = Join-Path ([IO.Path]::GetTempPath()) ('vibraPilot-' + [guid]::NewGuid().ToString('N'))
  try {
    Copy-Item -LiteralPath (Join-Path $Pilot 'robustness/authority') -Destination $work -Recurse
    $check = Invoke-Vibra $work 'check' $TimeLimit
    $test = Invoke-Vibra $work 'test' $TimeLimit
  }
  finally {
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
  }
  $check = (Get-Outcome $check).Document
  $test = (Get-Outcome $test).Document
  $unrestored = $check.status -eq 'passed' -and $test.status -eq 'passed'
  $verdict = if ($unrestored) { 'as registered' } else { 'NOT AS REGISTERED: expected passed when not restored' }
  "authorityUnrestored`treader`tendlet`tcheck=$($check.status)`ttest=$($test.status)`t$verdict"
  $total = $subjects.Count + 1
  if ($unrestored) { $met++ }
  "summary`t$met of $total subjects as registered; $($problems.Count) registration problems"
  $script:ExitCode = if ($met -eq $total -and $problems.Count -eq 0) { 0 } else { 1 }
}

# The reader against the corpus: a source that a case expects no syntax
# diagnostic for must be read, and every ordinary let of it must be closed;
# a source with an expected syntax diagnostic must not be read. Each source
# read is also stripped of its closers and translated back, which must give
# its tokens again.
$ReaderExceptions = @{
  'conformance/cases/PROJECT/V2-PROJECT-test-takes-other-type/tree/tests/run.vib' =
    'the checker, not the reader, reports a test parameter whose type is not TestWorld (01 "test")'
  'conformance/cases/TYPE-CONTROL/V2-TYPE-CONTROL-let-else-not-never/tree/src/app/main.vib' =
    'an endlet right after an ordinary let at the end of a fallback closes that let (registration, "Grading", step 1), so the file without its closers reads another way'
}

function Invoke-CheckReader {
  $files = @(Get-ChildItem -LiteralPath (Join-Path $Repository 'conformance/cases'), (Join-Path $Repository 'stdlib'), (Join-Path $Repository 'examples') -Recurse -File -Filter '*.vib' |
      Where-Object { $_.FullName -notlike "$Pilot*" })
  $counts = [ordered]@{ read = 0; refused = 0; skipped = 0; explained = 0; mismatched = 0 }
  foreach ($file in $files) {
    $name = $file.FullName.Substring($Repository.Length + 1).Replace('\', '/')
    if ($ReaderExceptions.ContainsKey($name)) { $counts.explained++; "EXPLAINED`t$name`t$($ReaderExceptions[$name])"; continue }
    $expectRead = $true
    $directory = $file.Directory
    while ($null -ne $directory -and -not (Test-Path -LiteralPath (Join-Path $directory.FullName 'case.toml'))) { $directory = $directory.Parent }
    if ($null -ne $directory -and $directory.FullName.StartsWith((Join-Path $Repository 'conformance'))) {
      $case = [IO.File]::ReadAllText((Join-Path $directory.FullName 'case.toml'))
      $relative = $file.FullName.Substring($directory.FullName.Length + 1).Replace('\', '/')
      # These operations read no source, and a data input is not source.
      if ($case -match '(?m)^operation = "(sourceGraph|projectDecode)"') { $counts.skipped++; continue }
      if ($case -match '(?m)^data = \[(.*)\]' -and $Matches[1].Contains("`"$relative`"")) { $counts.skipped++; continue }
      # The expected output of a format case keeps the bytes of a recovered
      # input, so it reads as that input does.
      $formatted = $case -match '(?m)^formatted = "([^"]+)"' -and $Matches[1] -eq $relative
      $blocks = $case -split '\[\[expect\.diagnostics\]\]' | Select-Object -Skip 1
      $unscoped = $false
      foreach ($block in $blocks) {
        if ($block -notmatch 'code = "(@[^"]+)"') { continue }
        $code = $Matches[1]
        if (-not $code.StartsWith('@syntax.')) { continue }
        if ($formatted) { $expectRead = $false; continue }
        if ($block -match '(?m)^source = "([^"]+)"') {
          $source = $Matches[1]
          if ($relative -eq $source -or $relative.EndsWith("/$source")) { $expectRead = $false }
        }
        elseif ($case -match '(?m)^source = "([^"]+)"' -and $Matches[1] -eq $relative) {
          # A diagnostic with no source is at the case's one input source.
          $expectRead = $false
        }
        else { $unscoped = $true }
      }
      if ($unscoped) { $counts.skipped++; continue }
    }
    $bytes = [IO.File]::ReadAllBytes($file.FullName)
    $library = $file.FullName.StartsWith((Join-Path $Repository 'stdlib'))
    $stripped = [VibraPilot.Reader]::Strip($bytes, $library)
    if (-not $stripped.Read) {
      if ($expectRead) { $counts.mismatched++; "MISMATCH`t$name`tnot read: $($stripped.Problem)" }
      else { $counts.refused++ }
      continue
    }
    if (-not $expectRead) { $counts.mismatched++; "MISMATCH`t$name`tread, but a syntax diagnostic is expected"; continue }
    $back = [VibraPilot.Reader]::Translate($stripped.Output, $library)
    $same = $back.Read -and $back.Closed -eq 0 -and $back.Ordinary -eq $stripped.Ordinary -and
      [VibraPilot.Reader]::Tokens($back.Output, $library) -eq [VibraPilot.Reader]::Tokens($bytes, $library)
    if ($stripped.Closed -ne $stripped.Ordinary -or -not $same) {
      $counts.mismatched++
      "MISMATCH`t$name`tordinary $($stripped.Ordinary), closed $($stripped.Closed), round trip $same"
      continue
    }
    $counts.read++
  }
  "summary`t$($files.Count) files: $($counts.read) read and round-tripped, $($counts.refused) refused as expected, $($counts.skipped) skipped (no source operation, a data input, or a syntax diagnostic with no source), $($counts.explained) explained, $($counts.mismatched) mismatched"
  $script:ExitCode = if ($counts.mismatched -eq 0) { 0 } else { 1 }
}

# The run records of step 40 against the registration. Each check is written
# here again, apart from the runner, from the registration's sections
# "Schedule", "Digests", "Extraction", "Grading" (regrading), and "Budget".
function Read-KeyValues([string]$Path) {
  $fields = @{}
  foreach ($line in [IO.File]::ReadAllLines($Path)) {
    if ($line -eq '') { continue }
    $key, $value = $line -split "`t", 2
    $fields[$key] = $value
  }
  return $fields
}

# "Extraction": the lines after the first line that begins with three
# backticks, up to the next such line or the end; else the whole text.
function Get-Extracted([byte[]]$Completion) {
  $lines = [Collections.Generic.List[int[]]]::new()
  $start = 0
  for ($i = 0; $i -lt $Completion.Length; $i++) {
    if ($Completion[$i] -eq 10) { $lines.Add(@($start, $i + 1)); $start = $i + 1 }
  }
  if ($start -lt $Completion.Length) { $lines.Add(@($start, $Completion.Length)) }
  $isFence = { param($line) $line[1] - $line[0] -ge 3 -and $Completion[$line[0]] -eq 0x60 -and $Completion[$line[0] + 1] -eq 0x60 -and $Completion[$line[0] + 2] -eq 0x60 }
  $first = -1
  for ($k = 0; $k -lt $lines.Count; $k++) { if (& $isFence $lines[$k]) { $first = $k; break } }
  if ($first -lt 0) { return , $Completion }
  $from = if ($first + 1 -lt $lines.Count) { $lines[$first + 1][0] } else { $Completion.Length }
  $to = $Completion.Length
  for ($k = $first + 1; $k -lt $lines.Count; $k++) { if (& $isFence $lines[$k]) { $to = $lines[$k][0]; break } }
  $bytes = [byte[]]::new($to - $from)
  [Array]::Copy($Completion, $from, $bytes, 0, $to - $from)
  return , $bytes
}

function Invoke-VerifyRuns([string]$TablePath) {
  $tableFile = [IO.Path]::GetFullPath($TablePath, (Get-Location).Path)
  $records = Join-Path (Split-Path $tableFile) 'runs'
  $problems = [Collections.Generic.List[string]]::new()
  $lines = Get-RegistrationLines
  $text = [IO.File]::ReadAllText((Join-Path $Repository 'docs/roadmap/v0.0.2/m2/pilot/registration.md'))
  $tasks = @($lines | Where-Object { $_ -match '^\| T\d+ \| `[a-z]+` \| `@pilot\.' } | ForEach-Object { ($_ -split '`')[1] })
  $models = @($lines | Where-Object { $_ -match '^\| `(haiku|luna)` \| ' } | ForEach-Object {
      $cells = $_.Split('|') | ForEach-Object { $_.Trim().Replace('`', '') }
      [pscustomobject]@{ Key = $cells[1]; Identifier = $cells[3]; Setting = $cells[4] }
    })
  $digests = @{}
  foreach ($line in $lines) {
    if ($line -match '^\| T\d+ `([a-z]+)` \| `(endlet|noEndlet)` \| \d+ \| `([0-9a-f]{64})` \|$') { $digests["$($Matches[1])/$($Matches[2])"] = $Matches[3] }
  }
  if ($tasks.Count -ne 10 -or $models.Count -ne 2 -or $digests.Count -ne 20) { throw 'the registration does not give ten tasks, two models, and twenty prompt digests' }
  if (-not ($text -replace '\s+', ' ').Contains('followed by the identifier, skipping runs with no completion')) { throw 'the registration no longer gives the regrade rule' }
  if ($text -notmatch 'UTF-8 text `(vibraPilot\d+:)`') { throw 'no regrade seed in the registration' }
  $seed = $Matches[1]

  # The schedule: per model, attempts 1 to 5, tasks T1 to T10, endlet first
  # in an odd attempt and noEndlet first in an even one.
  $schedule = [Collections.Generic.List[object]]::new()
  foreach ($model in $models) {
    $position = 0
    for ($attempt = 1; $attempt -le 5; $attempt++) {
      foreach ($task in $tasks) {
        $arms = if ($attempt % 2) { @('endlet', 'noEndlet') } else { @('noEndlet', 'endlet') }
        foreach ($arm in $arms) {
          $position++
          $schedule.Add([pscustomobject]@{
              Index = $schedule.Count + 1; Cohort = $model.Key; Position = $position; Run = "$($model.Key)-$task-$arm-$attempt"
              Model = $model.Identifier; Setting = $model.Setting; Task = $task; Arm = $arm; Attempt = $attempt; Digest = $digests["$task/$arm"]
            })
        }
      }
    }
  }
  $byRun = @{}
  foreach ($entry in $schedule) { $byRun[$entry.Run] = $entry }

  # The schedule file is the registration's schedule.
  $scheduleFile = Join-Path $records 'schedule.tsv'
  if (-not (Test-Path -LiteralPath $scheduleFile)) { $problems.Add('no schedule file runs/schedule.tsv') }
  else {
    $written = @([IO.File]::ReadAllLines($scheduleFile) | Select-Object -Skip 1 | Where-Object { $_ -ne '' })
    $expected = @($schedule | ForEach-Object { @($_.Index, $_.Cohort, $_.Position, $_.Run, $_.Model, $_.Setting, $_.Task, $_.Arm, $_.Attempt, $_.Digest) -join "`t" })
    if (($written -join "`n") -cne ($expected -join "`n")) { $problems.Add('runs/schedule.tsv is not the schedule of the registration') }
  }

  # Every record directory is a scheduled run.
  foreach ($directory in (Get-ChildItem -LiteralPath $records -Directory)) {
    if ($directory.Name -eq 'setup') { continue }
    if (-not $byRun.ContainsKey($directory.Name)) { $problems.Add("an extra attempt: the record $($directory.Name) has no schedule entry") }
  }

  # runs.tsv: one line per scheduled run, in schedule order.
  $table = @()
  if (Test-Path -LiteralPath $tableFile) { $table = @([IO.File]::ReadAllLines($tableFile) | Select-Object -Skip 1 | Where-Object { $_ -ne '' }) }
  else { $problems.Add('no runs.tsv') }
  for ($i = 0; $i -lt $table.Count; $i++) {
    $run = ($table[$i] -split "`t")[1]
    if ($i -ge $schedule.Count -or $run -ne $schedule[$i].Run) { $problems.Add("runs.tsv line $($i + 1) is $run, not the scheduled run $(if ($i -lt $schedule.Count) { $schedule[$i].Run } else { '(none)' })"); break }
  }

  $states = @{}
  $complete = 0
  foreach ($entry in $schedule) {
    $directory = Join-Path $records $entry.Run
    $recordFile = Join-Path $directory 'record.tsv'
    if (-not (Test-Path -LiteralPath $recordFile)) { $problems.Add("a missing attempt: $($entry.Run) has no record"); continue }
    $record = Read-KeyValues $recordFile
    $before = $problems.Count
    foreach ($pair in @(@('kind', 'run'), @('run', $entry.Run), @('index', "$($entry.Index)"), @('cohort', $entry.Cohort), @('task', $entry.Task), @('arm', $entry.Arm), @('attempt', "$($entry.Attempt)"), @('requestedModel', $entry.Model), @('setting', $entry.Setting))) {
      if ($record[$pair[0]] -cne $pair[1]) { $problems.Add("$($entry.Run): the record's $($pair[0]) is '$($record[$pair[0]])', not '$($pair[1])'") }
    }
    if ($record['promptSha256'] -cne $entry.Digest) { $problems.Add("$($entry.Run): the prompt digest is not the one registered for $($entry.Task) $($entry.Arm)") }
    $calls = @()
    $callsFile = Join-Path $directory 'calls.tsv'
    if (Test-Path -LiteralPath $callsFile) { $calls = @([IO.File]::ReadAllLines($callsFile) | Select-Object -Skip 1 | Where-Object { $_ -ne '' } | ForEach-Object { , ($_ -split "`t") }) }
    if ("$($calls.Count)" -ne $record['calls']) { $problems.Add("$($entry.Run): calls.tsv has $($calls.Count) calls, the record $($record['calls'])") }
    if ($calls.Count -gt 2) { $problems.Add("$($entry.Run): more than one retry") }
    $outcomes = @()
    for ($c = 0; $c -lt $calls.Count; $c++) {
      $call = $calls[$c]
      if ($call[0] -ne "$($c + 1)") { $problems.Add("$($entry.Run): calls are not numbered in order") }
      $outcomes += $call[6]
      $notSent = $call[7] -like 'interrupted*' -or $call[7] -eq 'the context audit failed'
      $stdout = Join-Path $directory "call-$($c + 1).stdout.txt"
      if (-not $notSent -and -not (Test-Path -LiteralPath $stdout)) { $problems.Add("$($entry.Run): call $($c + 1) has no output kept") }
      if ($call[6] -eq 'completion' -and $call[7] -notlike 'interrupted*' -and (Get-Item -LiteralPath $stdout).Length -eq 0) { $problems.Add("$($entry.Run): call $($c + 1) gave a completion and kept no output") }
      if ($c -lt $calls.Count - 1 -and $call[6] -ne 'failed') { $problems.Add("$($entry.Run): a call after a completion") }
    }
    $completionFile = Join-Path $directory 'completion.txt'
    $outcome = $record['outcome']
    switch ($outcome) {
      'completion' {
        if ($outcomes.Count -eq 0 -or $outcomes[-1] -ne 'completion') { $problems.Add("$($entry.Run): a completion whose last call did not give one") }
        if (-not (Test-Path -LiteralPath $completionFile)) { $problems.Add("$($entry.Run): no completion kept"); break }
        $completion = [IO.File]::ReadAllBytes($completionFile)
        if ((Get-Sha256 $completion) -ne $record['completionSha256']) { $problems.Add("$($entry.Run): completion.txt is not the recorded completion") }
        $solutionFile = Join-Path $directory 'solution.vib'
        # Assigned directly, so that an empty solution stays an empty array.
        $solution = $null
        if (Test-Path -LiteralPath $solutionFile) { $solution = [IO.File]::ReadAllBytes($solutionFile) }
        $extracted = Get-Extracted $completion
        if ($null -eq $solution -or (Get-Sha256 $solution) -ne (Get-Sha256 $extracted) -or (Get-Sha256 $solution) -ne $record['solutionSha256']) { $problems.Add("$($entry.Run): solution.vib is not the extraction of the completion") }
        $gradeFile = Join-Path $directory 'grade.txt'
        if (-not (Test-Path -LiteralPath $gradeFile)) { $problems.Add("$($entry.Run): no grade kept"); break }
        $gradeLine = [IO.File]::ReadAllText($gradeFile).TrimEnd("`n")
        $fields = $gradeLine -split "`t"
        if ($fields[0] -ne $entry.Task -or $fields[1] -ne $entry.Arm) { $problems.Add("$($entry.Run): the grade is of another task or arm") }
        $states[$entry.Run] = @{ Outcome = 'completion'; Grade = $fields[2]; Line = $gradeLine; Solution = $solution }
      }
      'providerError' {
        if ($calls.Count -ne 2 -or ($outcomes | Where-Object { $_ -ne 'failed' })) { $problems.Add("$($entry.Run): a provider error needs two failed calls") }
        if (Test-Path -LiteralPath $completionFile) { $problems.Add("$($entry.Run): a provider error with a completion") }
        $states[$entry.Run] = @{ Outcome = 'providerError'; Grade = 'providerError' }
      }
      'stopped' {
        if (Test-Path -LiteralPath $completionFile) { $problems.Add("$($entry.Run): a stopped run with a completion") }
        $states[$entry.Run] = @{ Outcome = 'stopped'; Grade = 'stopped' }
      }
      default { $problems.Add("$($entry.Run): the outcome '$outcome' is not final") }
    }
    if ($states.ContainsKey($entry.Run)) { $states[$entry.Run].Outcomes = $outcomes }
    # The table line holds the record's outcome and the grader's line.
    $line = if ($entry.Index - 1 -lt $table.Count) { $table[$entry.Index - 1] -split "`t" } else { $null }
    if ($null -eq $line) { $problems.Add("$($entry.Run): no line in runs.tsv") }
    elseif ($states.ContainsKey($entry.Run)) {
      if ($line[1] -ne $entry.Run -or $line[7] -ne $record['promptSha256'] -or $line[8] -ne $record['calls'] -or $line[9] -ne $outcome -or $line[12] -ne $states[$entry.Run].Grade) { $problems.Add("$($entry.Run): its line in runs.tsv does not match its record") }
    }
    if ($problems.Count -eq $before -and $null -ne $line) { $complete++ }
  }

  # The retry and stop rules ("Budget"), cohort by cohort in schedule order.
  $summary = [Collections.Generic.List[string]]::new()
  foreach ($model in $models) {
    $consecutive = 0; $made = 0; $retries = 0; $failedCalls = 0; $stoppedAt = ''
    foreach ($entry in ($schedule | Where-Object Cohort -eq $model.Key)) {
      if (-not $states.ContainsKey($entry.Run)) { continue }
      $state = $states[$entry.Run]
      if ($consecutive -ge 5) {
        if ($state.Outcome -ne 'stopped' -or $state.Outcomes.Count) { $problems.Add("$($entry.Run): a call after the cohort stopped") }
        continue
      }
      $n = 0
      foreach ($outcome in $state.Outcomes) {
        $n++
        if ($consecutive -ge 5) { $problems.Add("$($entry.Run): a call after five consecutive provider failures") }
        $made++
        if ($n -eq 2) { $retries++ }
        if ($outcome -eq 'failed') { $consecutive++; $failedCalls++ } else { $consecutive = 0 }
      }
      if ($state.Outcome -eq 'stopped') {
        if ($consecutive -lt 5) { $problems.Add("$($entry.Run): stopped without five consecutive provider failures") }
        elseif ($stoppedAt -eq '') { $stoppedAt = $entry.Run }
      }
    }
    # The setup calls: at most five, outside the runs, one passed before the
    # cohort's first call.
    $setups = @(Get-ChildItem -LiteralPath (Join-Path $records 'setup') -Directory -ErrorAction SilentlyContinue | Where-Object Name -match "^$($model.Key)-\d+$")
    $passed = @($setups | Where-Object { (Test-Path (Join-Path $_.FullName 'record.tsv')) -and (Read-KeyValues (Join-Path $_.FullName 'record.tsv'))['setupPassed'] -eq 'true' })
    if ($setups.Count -gt 5) { $problems.Add("$($model.Key): more than five setup calls") }
    $first = $schedule | Where-Object Cohort -eq $model.Key | Select-Object -First 1
    $firstCalls = Join-Path (Join-Path $records $first.Run) 'calls.tsv'
    if ($made -gt 0) {
      if ($passed.Count -eq 0) { $problems.Add("$($model.Key): runs were called with no passed setup call") }
      elseif (Test-Path -LiteralPath $firstCalls) {
        $firstStart = ([IO.File]::ReadAllLines($firstCalls)[1] -split "`t")[1]
        $setupEnd = (Read-KeyValues (Join-Path $passed[0].FullName 'record.tsv'))['endUtc']
        if ([string]::CompareOrdinal($setupEnd, $firstStart) -gt 0) { $problems.Add("$($model.Key): the setup call passed after the first run's call") }
      }
    }
    $stopText = if ($stoppedAt) { "; stopped by the stop rule at $stoppedAt" } else { '' }
    $summary.Add("cohort $($model.Key): $made calls ($retries retries, $failedCalls failed at the provider); $($setups.Count) setup calls$stopText")
  }

  # The regrade: the first 20 runs with a completion when the identifiers are
  # sorted by the SHA-256 of the seed followed by the identifier.
  $vibraSha = Get-Sha256 ([IO.File]::ReadAllBytes($script:VibraPath))
  $keyed = [Collections.Generic.List[object]]::new()
  foreach ($entry in $schedule) { $keyed.Add([pscustomobject]@{ Key = Get-Sha256 ([Text.Encoding]::UTF8.GetBytes("$seed$($entry.Run)")); Entry = $entry }) }
  $keyed.Sort([Comparison[object]] { param($x, $y) [string]::CompareOrdinal($x.Key, $y.Key) })
  $order = @($keyed | ForEach-Object { $_.Entry })
  $picked = @($order | Where-Object { $states.ContainsKey($_.Run) -and $states[$_.Run].Outcome -eq 'completion' } | Select-Object -First 20)
  $sameGrade = 0; $sameLine = 0
  foreach ($entry in $picked) {
    $state = $states[$entry.Run]
    try { $again = Format-Grade (Get-Grade $entry.Task $entry.Arm $state.Solution) }
    catch { $again = "$($entry.Task)`t$($entry.Arm)`tgraderError" }
    $grade = ($again -split "`t")[2]
    if ($grade -ceq $state.Grade) { $sameGrade++ } else { $problems.Add("regrade: $($entry.Run) was $($state.Grade) and is now $grade") }
    if ($again -ceq $state.Line) { $sameLine++ }
    "regrade`t$($entry.Run)`t$grade`t$(if ($again -ceq $state.Line) { 'same line' } else { 'same grade, another line' })"
  }
  if ($picked.Count -lt 20 -and @($states.Values | Where-Object { $_.Outcome -eq 'completion' }).Count -ge 20) { $problems.Add('regrade: fewer than 20 runs picked') }

  $counts = @{ completion = 0; providerError = 0; stopped = 0 }
  foreach ($state in $states.Values) { $counts[$state.Outcome]++ }
  foreach ($line in $summary) { $line }
  "outcomes`t$($counts.completion) with a completion, $($counts.providerError) providerError, $($counts.stopped) stopped"
  "regrade`t$sameGrade of $($picked.Count) the same grade; $sameLine of $($picked.Count) the same line"
  $recorded = @($schedule | Where-Object { Test-Path (Join-Path (Join-Path $records $_.Run) 'record.tsv') } | ForEach-Object { (Read-KeyValues (Join-Path (Join-Path $records $_.Run) 'record.tsv'))['vibraSha256'] } | Sort-Object -Unique)
  "grader`tvibra sha256 $vibraSha; the records name $($recorded -join ', ')"
  foreach ($problem in $problems) { "PROBLEM`t$problem" }
  if ($problems.Count -eq 0 -and $complete -eq $schedule.Count -and $table.Count -eq $schedule.Count) {
    "schedule complete: $complete of $($schedule.Count)"
    $script:ExitCode = 0
  }
  else {
    "schedule not complete: $complete of $($schedule.Count) runs verified; $($problems.Count) problems"
    $script:ExitCode = 1
  }
}

# The translation alone over the stored noEndlet solutions of the runs: what
# the translation changed. No vibra runs and nothing is graded.
function Invoke-TranslationCounts([string]$TablePath) {
  $tableFile = [IO.Path]::GetFullPath($TablePath, (Get-Location).Path)
  $records = Join-Path (Split-Path $tableFile) 'runs'
  $rows = @(Import-Csv -LiteralPath $tableFile -Delimiter "`t" | Where-Object arm -ceq 'noEndlet')
  $problems = [Collections.Generic.List[string]]::new()
  $failedRuns = [Collections.Generic.List[string]]::new()
  $counts = [ordered]@{}
  foreach ($row in $rows) {
    if (-not $counts.Contains($row.cohort)) { $counts[$row.cohort] = [ordered]@{ read = 0; changed = 0; unchanged = 0; failed = 0 } }
    $directory = Join-Path $records $row.run
    $file = Join-Path $directory 'solution.vib'
    if (-not (Test-Path -LiteralPath $file)) {
      if ($row.outcome -eq 'completion') { $problems.Add("$($row.run): no stored solution") }
      continue
    }
    $bytes = [IO.File]::ReadAllBytes($file)
    $recorded = (Read-KeyValues (Join-Path $directory 'record.tsv'))['solutionSha256']
    if ((Get-Sha256 $bytes) -ne $recorded) { $problems.Add("$($row.run): the stored solution is not the one its record names") }
    $translation = [VibraPilot.Reader]::Translate($bytes)
    $closed = if ($translation.Read) { $translation.Closed } else { 0 }
    if ("$closed" -ne $row.closed) { $problems.Add("$($row.run): closed count $closed, the run table has $($row.closed)") }
    $c = $counts[$row.cohort]
    $c.read++
    if (-not $translation.Read) { $c.failed++; $failedRuns.Add($row.run) }
    elseif ($translation.Changed) { $c.changed++ }
    else { $c.unchanged++ }
  }
  "model`tread`tchanged`tunchanged`treadingFailed"
  $all = [ordered]@{ read = 0; changed = 0; unchanged = 0; failed = 0 }
  foreach ($model in $counts.Keys) {
    $c = $counts[$model]
    "$model`t$($c.read)`t$($c.changed)`t$($c.unchanged)`t$($c.failed)"
    foreach ($key in @($all.Keys)) { $all[$key] += $c[$key] }
  }
  "all`t$($all.read)`t$($all.changed)`t$($all.unchanged)`t$($all.failed)"
  foreach ($run in $failedRuns) { "readingFailed`t$run" }
  foreach ($problem in $problems) { "problem`t$problem" }
  "summary`t$($all.read) noEndlet solutions read; $($problems.Count) problems"
  $script:ExitCode = if ($problems.Count) { 1 } else { 0 }
}

switch ($PSCmdlet.ParameterSetName) {
  'Counts' {
    Invoke-TranslationCounts $TranslationCounts
    exit $script:ExitCode
  }
  'Verify' {
    $script:VibraPath = Resolve-VibraPath
    Invoke-VerifyRuns $VerifyRuns
    exit $script:ExitCode
  }
  'Reader' {
    Invoke-CheckReader
    exit $script:ExitCode
  }
  'All' {
    $script:VibraPath = Resolve-VibraPath
    Invoke-All
    exit $script:ExitCode
  }
  'One' {
    $script:VibraPath = Resolve-VibraPath
    $path = [IO.Path]::GetFullPath($File, (Get-Location).Path)
    Format-Grade (Get-Grade $Task $Arm ([IO.File]::ReadAllBytes($path)))
    exit 0
  }
}