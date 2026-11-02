# C# oracle

Roslyn's own lexer and parser, run on a list of files, printing what the Rust port (`crates/csharp-syntax`,
`ksdump --lang csharp`) must print byte for byte. Roslyn is the NuGet package `Microsoft.CodeAnalysis.CSharp`
5.9.0, built from dotnet/roslyn `35d9211b841e7613c1d2f8f5af6d628ace696c4c` (branch release/insiders; the commit
its nuspec names, which no tag contains). The dump is not imitated: it is a walk of the red tree Roslyn returns,
`ChildNodesAndTokens()` with each token's trivia, the order of Roslyn's own `ParsingTests.EnumerateNodes`.

| file | does |
|---|---|
| `CSharpOracle.csproj`, `global.json`, `nuget.config`, `Directory.Build.props` | a `net10.0` console program on `Microsoft.CodeAnalysis.CSharp` `[5.9.0]`, SDK 10.0.403 exactly (`rollForward: disable`), nuget.org only; build products and packages in `~/.cache/psi-syntax/csharp-oracle/` |
| `packages.lock.json` | the content hash of every package (`RestoreLockedMode`): restore fails when a package does not match, as `jars.sha256` pins the Java oracle's jars. `CSHARP_ORACLE_UNLOCK=1` rewrites it (a pin bump) |
| `oracle.sh` | builds the program once per source tree (`build/<hash of the sources>`) and runs it: `oracle.sh MODE ARGS` |
| `Program.cs`, `Dump.cs`, `Values.cs` | the modes `dump`, `lex`, `values` and `unicode`, the dump format below |
| `Goldens.cs` | the `goldens` mode: the inputs of Roslyn's parser and lexer tests |
| `vendor-goldens.sh` | writes `testdata/csharp/` from the tests at the pin (`git archive`, never a working tree) |
| `corpus.py` | a corpus list: the tracked `.cs` files of a checkout that are valid UTF-8 |
| `caches.sh` | rebuilds every cache below (clones, lists, dumps, lexer lines, fuzz) |
| `link_same.py` | hard-links the Preview dumps that equal the Latest ones |
| `mutate.py` | one-place mutations of the diagnostic-free goldens, after `tools/java-oracle/mutate.py` |
| `lex_fuzz.py` | lexer fuzz inputs: the golden inputs with lexer-state pieces inserted, replaced or deleted |
| `check_offsets.py` | checks a dump directory against the source bytes: every token and trivia text at its UTF-8 offsets |

The SDK lives in `~/.cache/psi-syntax/dotnet` (`dotnet-install.sh --version 10.0.403 --install-dir ...`, no
system install); `oracle.sh` sets `DOTNET_ROOT` to it unless set. The Unicode tables are those of that runtime
(.NET 10.0.x), which `CharUnicodeInfo` and so Roslyn's `CharacterInfo` read.

## Running it

```sh
tools/csharp-oracle/oracle.sh dump --list LIST --root ROOT --out OUT [--version latest|preview|N] \
  [--doc parse|none|diagnose] [--entry NAME] [--threads N] [--stack-mb M]
tools/csharp-oracle/oracle.sh dump --serve [the same defaults]   # stdin: LIST<TAB>ROOT<TAB>OUT, prints done after each
tools/csharp-oracle/oracle.sh lex --list LIST --root ROOT [--version ...] [--doc ...] [--full OUT]
tools/csharp-oracle/oracle.sh values --list LIST --root ROOT [--version ...] [--doc ...] [--entry ...] [--kept]
tools/csharp-oracle/oracle.sh goldens --src ROSLYN_TREE --out DIR [--skipped LOG]   # vendor-goldens.sh runs it
tools/csharp-oracle/oracle.sh unicode   # CharUnicodeInfo.GetUnicodeCategory of 0..10FFFF as runs
```

Files are parsed in parallel (`--threads`, default the processor count), each on a worker thread of one stack size
(`--stack-mb`, default 64): Roslyn's stack guard (`EnsureSufficientExecutionStack`) depends on the stack left, so
one size for every file makes the output the same at any thread count. Every output file is written whole and
renamed into place.

## The dump (normative)

The oracle and `ksdump --lang csharp` print exactly this. When a case below needs a decision, the oracle's
behaviour is fixed first and recorded here; the Rust side follows.

### Input and options

- The file's bytes are decoded as strict UTF-8 (`new UTF8Encoding(false, true).GetString`, the same text as
  `Encoding.UTF8.GetString` for valid input): a BOM stays the char U+FEFF, which Roslyn lexes as `WhitespaceTrivia`.
  Only valid UTF-8 is dumped; an invalid file gets no dump and the index line `"failure":"invalid UTF-8"`.
- `new CSharpParseOptions(languageVersion, documentationMode, SourceCodeKind.Regular)`, no preprocessor symbols, no
  features. `--version`: anything `LanguageVersionFacts.TryParse` takes (`latest`, `preview`, `default`,
  `latestmajor`, `7.3`, `13`, `13.0`); default `latest`. `--doc`: `parse` (default, as `CSharpParseOptions.Default`:
  doc comments are structured trivia), `none` (plain comment trivia) or `diagnose` (structured, and the XML errors of
  a doc comment are reported).
- `--entry` (default `compilation-unit`), each with those options and `consumeFullText: true` (Roslyn attaches the
  rest of the text after the root as skipped-tokens trivia with an error; on a few inputs a partial entry's root
  does not spell the whole text, 70 of the 122 741 mutations, and the dump is what Roslyn returns all the same):

  | entry | call |
  |---|---|
  | `compilation-unit` | `CSharpSyntaxTree.ParseText(text, options).GetRoot()` |
  | `expression` | `SyntaxFactory.ParseExpression(text, 0, options, true)` |
  | `statement` | `SyntaxFactory.ParseStatement(text, 0, options, true)` |
  | `member` | `SyntaxFactory.ParseMemberDeclaration(text, 0, options, true)`; when it returns null the file is a failure, `"no member"` |
  | `type-name` | `SyntaxFactory.ParseTypeName(text, 0, options, true)` |
  | `name` | `SyntaxFactory.ParseName(text, 0, true)`: it takes no options, so it always parses with `CSharpParseOptions.Default` (latest, parse) |
  | `parameter-list`, `bracketed-parameter-list`, `argument-list`, `bracketed-argument-list`, `attribute-argument-list` | `SyntaxFactory.Parse<Kind>(text, 0, options, true)` |
  | `tokens` | `SyntaxFactory.ParseTokens(text, options: options)`: the lexer lines below, as the whole dump |

- Out of scope, as accio needs them: scripts (`.csx`), preprocessor symbols, `Features` (`FileBasedProgram` makes
  `#:` directives legal), incremental parsing.

### Offsets

Every offset is a UTF-8 byte offset into the file. The oracle maps Roslyn's UTF-16 positions to bytes: a char
below U+0080 is 1 byte, below U+0800 2, any other BMP char 3, and each half of a surrogate pair 2. So a position
between the two halves of one supplementary character maps to that character's byte offset + 2 (Roslyn reports
such positions: `"\😀"` gives CS1009 `[22..25)`, the backslash and the high surrogate). Valid UTF-8 never decodes to
a lone surrogate.

### Lines

One line per element, in `ChildNodesAndTokens()` order, recursively, two spaces of indentation per depth; lists
are flattened and null slots skipped exactly as `ChildNodesAndTokens()` does. `Kind` is `SyntaxKind.ToString()`
(575 kinds at the pin, no two with one value). The root is at depth 0.

- Node: `Kind [s..e)`, `s..e` the node's `FullSpan`.
- Token: first its leading trivia, each a trivia line at the token's depth prefixed `L:`; then
  `Kind [s..e) 'text'` with `s..e` the token's `Span` and `text` its `Text`; then its trailing trivia, prefixed `T:`.
  A missing token (`IsMissing`) prints `Kind [s..s) missing` in place of a text (its trivia lines as any token's).
  A token's text is `''` when empty (`EndOfFileToken [n..n) ''`, `EndOfDirectiveToken`, `EndOfDocumentationCommentToken`).
- Unstructured trivia: `L:Kind [s..e) 'text'` or `T:Kind [s..e) 'text'` (`FullSpan`, `ToFullString()`).
- Structured trivia (`HasStructure`: directives, doc comments, skipped tokens): `L:Kind [s..e)` or `T:Kind [s..e)`,
  then the structure node's children at depth + 1 (the structure node itself is not printed again; its kind is
  the trivia's).

```
CompilationUnit [0..34)
  ClassDeclaration [0..34)
    ClassKeyword [0..5) 'class'
    T:WhitespaceTrivia [5..6) ' '
    ...
        LocalDeclarationStatement [21..31)
          VariableDeclaration [21..29)
            ...
              EqualsValueClause [27..29)
                EqualsToken [27..28) '='
                T:WhitespaceTrivia [28..29) ' '
                IdentifierName [29..29)
                  IdentifierToken [29..29) missing
    ...
    CloseBraceToken [34..34) missing
  EndOfFileToken [34..34) ''
Diagnostics:
  CS1525 [29..30) Invalid expression term \';\'
  CS1513 [32..32) } expected
```

### Text escapes

In `'text'` and in a diagnostic's message: `\` is `\\`, `'` is `\'`, LF `\n`, CR `\r`, TAB `\t`; any other char
below U+0020, U+007F, U+0085, U+2028, U+2029, U+FEFF and a lone surrogate are `\u{XXXX}`, with the braces, the
code unit in 4 uppercase hex digits (`\u{FEFF}`, `\u{000B}`); every other char, a surrogate pair included, is
written as UTF-8.

### Diagnostics

After the tree, if there are any: a line `Diagnostics:` then one line per diagnostic, in the order Roslyn returns
them, `  CSnnnn [s..e) message`: `Id`, the location's `SourceSpan` in bytes, `GetMessage(CultureInfo.InvariantCulture)`
(the neutral English resources) with the text escapes; the one message that names a file of the machine, CS8304 of
`#error version`, has the compiler's directory removed (`Compiler version: '5.9.0-1.26357.3
(35d9211b841e7613c1d2f8f5af6d628ace696c4c)'. Language version: 4. Compiler path: 'Microsoft.CodeAnalysis.CSharp.dll'.`).
The diagnostics are `SyntaxTree.GetDiagnostics()` of the
compilation unit, `GetDiagnostics()` of the root of another entry: errors and warnings alike (`#warning` is CS1030),
duplicates included (Roslyn reports some twice, e.g. CS1519 for one bad token), zero-width ones as Roslyn makes them
(`CS1056 [73..73) Unexpected character`).

### Files

`--list LIST --root ROOT --out OUT`: each list line is `PATH[<TAB>VERSION[<TAB>ENTRY[<TAB>DOC]]]`, PATH relative to
ROOT; an empty or absent column takes the command's flag, more columns are ignored, empty lines are skipped. The
dump of PATH goes to `OUT/PATH.tree.txt` (UTF-8, every line ending with `\n`, so the file ends with one).
`OUT/index.jsonl` gets one line per list line, in list order: `{"path":P,"errors":N,"firstError":L}` where N counts
the diagnostic lines and L is the first one without its indentation (`"CS1525 [29..30) Invalid expression term \\';\\'"`)
or `null`; or `{"path":P,"failure":REASON}` without a dump: `invalid UTF-8`, `no member`, `dump over 512 MB` (a
dump longer than 2^29 chars: the 24 MB nested expressions of dotnet/runtime's `src/tests/JIT/jit64/opt/cse/`, whose
lines are mostly indentation), or an exception's type and message. The JSON escapes `"`, `\`, controls (`\n`,
`\r`, `\t`, else `\u00XX`); every other char is itself.

### Lexer lines (`lex`, `ksdump --lex --lang csharp`)

The tokens of `SyntaxFactory.ParseTokens(text, options: options)` (lexer mode `Syntax`, from position 0, the
`EndOfFileToken` included) each printed as a token above at depth 0 (its `L:`/`T:` trivia lines, structured trivia
children from depth 1), then `Diagnostics:` and their lines as above, if any. Tokens out of `ParseTokens` belong to
no tree, so their own `GetDiagnostics()` has no location: the oracle puts each token that has a diagnostic (its own
or its trivia's) back into one, as the `SkippedTokensTrivia` of an empty compilation unit's end-of-file token (the
end-of-file token itself as that token), and prints that tree's `GetDiagnostics()` with the spans moved by the
token's full start: per token, in token order, in the document order a parse reports them (all tokens in one such
tree gives the same lines, checked on 3000 lexer fuzz inputs at both documentation modes and on the goldens, but in
time quadratic in the token count). That text is the `tokens` entry's dump.

Each list line prints one line, in list order: `PATH<TAB>TOKENS<TAB>HASH`, TOKENS the token count and HASH the
FNV-1a 64-bit hash (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`) of the dump's UTF-8 bytes, 16 lowercase
hex digits; or `PATH<TAB>failure<TAB>REASON`. `--full OUT` also writes the dump to `OUT/PATH.lex.txt`. List lines
are read as for `dump` (VERSION and DOC columns apply, ENTRY is ignored).

### Value texts (`values`, `ksdump --values --lang csharp`)

One line per token whose `ValueText` is not its `Text`, in document order, the tokens of structured trivia
(directives, documentation comments, skipped tokens) included (`DescendantTokens(descendIntoTrivia: true)`; for the
`tokens` entry each lexed token after the tokens of its leading trivia's structures and before its trailing
trivia's): `PATH<TAB>START<TAB>Kind<TAB>'value'`, START the token's `SpanStart` in UTF-8 bytes, the value with the
text escapes; or `PATH<TAB>failure<TAB>REASON`. List lines as for `dump`. `--kept` leaves out the tokens whose value
the port does not keep (`Values.cs`, `Kept`: numeric, character, raw string and interpolated text literals, and
strings outside directives; `value_not_kept` of crates/csharp-syntax/src/tree.rs): what `Tree::value_text` answers,
the lines `ksdump --values` prints of ours. The offsets map Roslyn's positions through the source, so after a token
whose tree text is not the source's (`&#103;lobal::` in a cref becomes the keyword `global` of fixed text) a position
past a character outside ASCII maps to another byte than ours, which count the tree's own text.

The gate (`ksdump --values --check LINES`) compares ours with cached `--kept` lines, made once with the lists' own
columns (`C=~/.cache/psi-syntax`):

```sh
mkdir -p $C/values
for c in roslyn-35d9211:roslyn runtime-6aa72da:runtime aspnetcore-0ef4bbf:aspnetcore avalonia-2d79ba5:avalonia; do
  tools/csharp-oracle/oracle.sh values --kept --list $C/corpus/csharp-${c%%:*}.list --root $C/csharp-src/${c##*:} \
    > $C/values/csharp-${c%%:*}.kept.txt
done
tools/csharp-oracle/oracle.sh values --kept --list $C/fuzz/csharp-mut.list --root $C/fuzz/csharp-mut \
  > $C/values/csharp-mut.kept.txt
tools/csharp-oracle/oracle.sh values --kept --list $C/fuzz/csharp-lex/list --root $C/fuzz/csharp-lex \
  > $C/values/csharp-lex.kept.txt
```

`crates/csharp-syntax/tests/values/goldens.values.txt` is the same of the goldens (`manifest.tsv`'s lines with `.cs`
added to the path, `--root testdata/csharp`), and `values.values.txt` of `values.cs` beside it. Equal on 2026-11-02:
the four corpora, the 7051 goldens at their entries, the 122 741 mutations and the 20 000 lexer fuzz inputs (DESIGN,
"Value texts").

## Goldens (`testdata/csharp/`)

`goldens` parses `src/Compilers/CSharp/Test/Syntax/Parsing/*.cs` and `LexicalAndXml/*.cs` of the pin with Roslyn
(one compilation over the 82 files and the runtime's reference assemblies, for constant folding and symbol lookup)
and takes every call that parses a text:

- the helpers of `ParsingTests` (`UsingTree`, `UsingNode`, `UsingExpression`, `UsingStatement`, `UsingDeclaration`,
  `ParseTree`, `ParseFile`, `ParseAndValidate`, `ParseWithRoundTripCheck`), `SyntaxFactory.Parse*` and
  `CSharpSyntaxTree.ParseText`, and the private helpers of single classes (`LexicalTests.Lex`/`LexToken`,
  `PreprocessorTests.Parse`, `XmlDocCommentTests.Parse`, `RoundTrippingTests.ParseAndRoundTripping`, ...);
- with each class's overrides of `ParseTree`/`ParseNode` (`AsyncParsingTests` forces C# 5, `UsingDirectiveParsingTests`
  defaults to Preview, `CrefParsingTests` wraps the text in `/// <see cref="..."/>` and lexes it, which is the
  `tokens` entry of that text) and the `private new` wrappers that fix options (`RecordParsingTests`: C# 9);
- whose text is a constant: literals (regular, verbatim, raw), constants, concatenations, interpolations of constant
  parts (raw ones too; checked against the compiler's own folding on every constant interpolation of the tests),
  `SyntaxFacts.GetText(SyntaxKind.X)`, `string.Format`, a local's value at the call (its initializer or the last
  plain assignment before the call), and the rows of a theory (`[InlineData]`, `[MemberData]` over a
  `TheoryData`/array field, `[CombinatorialData]` with `[CombinatorialValues]` and bools) and of `foreach` loops over
  literal arrays;
- with options evaluated from `TestOptions.*` (its table at the pin), `CSharpParseOptions.Default`,
  `new CSharpParseOptions(...)` and `With*` chains.

Left out: scripts, preprocessor symbols (`Parse(text, "DEBUG")`), features, a text that is not valid UTF-8 (a lone
surrogate), a nonzero offset, `consumeFullText: false`, options it cannot evaluate, duplicates (same text, entry,
version and documentation mode: the first is kept), and a dump over 1 MB (`ExpressionParsingTests.ParseBigExpression`,
20 MB). `vendor-goldens.sh` prints the counts. Each golden is `testdata/csharp/NAME.cs` (the text's UTF-8),
`NAME.tree.txt` (its dump) and a `manifest.tsv` line `NAME<TAB>VERSION<TAB>ENTRY<TAB>DOC<TAB>SOURCE`: NAME is
`TestClass/Member` or `TestClass/Member_N` (a member with several inputs, numbered in source order, theory rows
included; `__2` on a clash, also one of case), VERSION `latest` for every version that maps to the latest release
(Default, Latest, LatestMajor, 14), `preview`, or the number (`5`, `7.3`, `13`), SOURCE the call's file and line.

## Measured (2026-11-02, Apple M5 Max, .NET SDK 10.0.403, 18 threads on a shared machine)

- Goldens (`vendor-goldens.sh`, 8 s): the 82 test files hold 5953 recognized calls, 7214 constant inputs with
  their theory and loop rows, 7051 goldens (9.8 MB of text and dumps): 3177 `compilation-unit`, 1236 `expression`,
  934 `member`, 773 `statement`, 892 `tokens`, 22 `name`, 16 `type-name`, 1 `attribute-argument-list`; 6282 at
  `parse`, 769 at `diagnose`; 3758 at `latest`, 1297 at `preview`, 1996 at 1 to 13; 3350 with a diagnostic. Left
  out: 428 script, 159 duplicates, 50 options not evaluated, 45 features, 14 preprocessor symbols, 3 `no member`,
  2 lone surrogates, 2 `consumeFullText: false`, 1 offset, 1 dump over 1 MB, and 329 calls whose text is not
  constant (helpers' own bodies, texts built by loops or by `GetFieldIdentifier(bool)`, aligned interpolations).
  A rerun at 1 and at 18 threads writes identical files.

| corpus | commit | `.cs` listed | bytes | with a diagnostic | failures | Latest dumps | on disk |
|---|---|---|---|---|---|---|---|
| dotnet/roslyn (the pin) | `35d9211b841e7613c1d2f8f5af6d628ace696c4c` | 17 075 of 17 076 (1 not UTF-8) | 249 938 192 | 41 | 0 | 6.7 s | 3.0 GB |
| dotnet/runtime `v10.0.14` | `6aa72daa24f66c01c3d7b6a32519e9d21396deec` | 31 267 of 31 268 (1 not UTF-8) | 426 882 167 | 29 | 6 `dump over 512 MB` | 34.1 s | 6.9 GB |
| dotnet/aspnetcore `v10.0.14` | `0ef4bbfa3291b306a21e5001cb0491277bdd35bc` | 10 188 | 62 042 224 | 16 | 0 | 12.7 s | 1.1 GB |
| AvaloniaUI/Avalonia `12.1.4` | `2d79ba55fae1b1e71c69f93a668080c9029e6a31` | 4 085 | 23 589 460 | 2 | 0 | 1.3 s | 0.41 GB |

- The six failures are the generated JIT stress tests of dotnet/runtime (`src/tests/JIT/jit64/opt/cse/Huge*.cs`,
  `hugeexpr1.cs`, `hugeSimpleExpr1.cs`, `JitBlue/GitHub_10215`): 8 to 24 MB of nested expressions whose dumps are
  mostly indentation; the four of them run in 64 s and 3.3 GB.
- Preview: every dump of the four corpora equals its Latest dump (all hard links, no extra bytes).
- Lexer lines of the four corpora at both documentation modes: 21 s, no failure, 45 MB with the fuzz's.
- Deterministic: Roslyn's dumps at 3 threads and Avalonia's at 1 equal those at 18 (`diff -rq`, 0 lines);
  aspnetcore's lexer lines at 1 thread equal those at 18.
- Offsets (`check_offsets.py`): 0 mismatches over every token and trivia text of the goldens (149 741), Roslyn
  (34 887 922), runtime (83 279 148), aspnetcore (13 069 984), Avalonia (5 201 042), the mutations (2 890 637) and
  the lexer fuzz (430 231, among them tokens of half a surrogate pair). 70 mutants of partial entries have a root
  that does not spell the text (Roslyn drops some, e.g. `required ` in `short required implicit operator C(S s) {}`
  parsed as a member): their dumps are Roslyn's all the same.
- Mutations (`mutate.py`, seed `csharp-mut`, 2 sites per operator and file): 3701 diagnostic-free goldens (3350 with
  one left out), 119 281 mutants, 122 982 files; 241 mutants of `member` goldens are `no member`, so the oracle list
  holds 122 741 (59 016 `compilation-unit`), 65 532 with a diagnostic; 15 s. Regenerating gives identical files.
- Lexer fuzz (`lex_fuzz.py`, 20 000 files, seed `csharp-lex`): 18 163 of their compilation units have a diagnostic.
- Disk: clones 2.0 GB, dumps 11.4 GB (Preview free), fuzz 1.2 GB, the oracle's builds and packages 255 MB.

## Caches (`~/.cache/psi-syntax`)

| path | holds |
|---|---|
| `dotnet/` | the .NET SDK 10.0.403 |
| `roslyn/` | the blobless clone of dotnet/roslyn the port reads its sources from (`git show 35d9211b:<path>`) |
| `csharp-oracle/nuget/`, `csharp-oracle/build/<hash>/` | the restored packages, the oracle's builds |
| `csharp-src/{roslyn,runtime,aspnetcore,avalonia}/` | the corpora, shallow checkouts (see "Measured" for the commits) |
| `corpus/csharp-REPO-SHA7.list` | each corpus list (`corpus.py`) |
| `oracle/csharp-REPO-SHA7/` | its dumps at Latest, Parse (`.tree.txt`, `index.jsonl`) |
| `oracle/csharp-REPO-SHA7-preview/` | its dumps at Preview, Parse; those equal to Latest's are hard links to them |
| `lex/csharp-REPO-SHA7.oracle.txt`, `lex/csharp-REPO-SHA7.doc.oracle.txt`, `lex/csharp-REPO-SHA7.diag.oracle.txt` | its lexer lines at `--doc none`, `--doc parse` and `--doc diagnose` |
| `fuzz/csharp-mut/` | the mutants, `original/`, `origins.json` and the full `list` (`mutate.py`, seed `csharp-mut`) |
| `fuzz/csharp-mut.list` | the oracle list: `list` without the mutants their entry cannot parse (`no member`) |
| `fuzz/csharp-mut.oracle/` | its dumps |
| `fuzz/csharp-mut-latest.list`, `fuzz/csharp-mut-preview.list` | the oracle list with every version `latest`, `preview` |
| `fuzz/csharp-mut-latest.oracle/`, `fuzz/csharp-mut-preview.oracle/` | their dumps; those equal to `csharp-mut.oracle`'s are hard links to them |
| `fuzz/csharp-lex/` | the lexer fuzz inputs and their `list` (`lex_fuzz.py`, 20 000 files, seed `csharp-lex`) |
| `fuzz/csharp-lex.oracle/` | their dumps (compilation units at Latest, Parse) |
| `fuzz/csharp-lex-preview.oracle/` | their dumps at Preview; those equal to Latest's are hard links to them |
| `lex/csharp-mut{,.doc,.diag}.oracle.txt`, `lex/csharp-lex{,.doc,.diag}.oracle.txt` | the lexer lines of both, at `--doc none`, `--doc parse` and `--doc diagnose`, with the defaults for every file (the lists' paths only) |

Rebuild them all with `tools/csharp-oracle/caches.sh` (steps `clones lists dumps lex fuzz`, any subset in that
order; it stops when less than 220 GB would be free), the goldens with `tools/csharp-oracle/vendor-goldens.sh`.
Compare a port's dumps with `scripts/compare.py ORACLE OURS` (it detects `.tree.txt`) and its lexer lines with
`diff` of the oracle's file.
