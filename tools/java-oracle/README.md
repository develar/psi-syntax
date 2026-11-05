# Java PSI oracle

IntelliJ's own Java parser, run on a list of files, printing what its Java parsing tests compare: the
reference for the Rust port of `java/java-syntax`. The parser is the one IntelliJ 2027.1
(build 271.SNAPSHOT) ships, whose java-syntax and platform/syntax equal JetBrains/intellij-community at
`decc943674edfe7ecc620d2d60b61be38f876994`: java-syntax on the platform syntax builder, converted to
real PSI by `PsiSyntaxBuilderFactoryImpl`, the same path the IDE takes. The dump is not imitated: it
is `DebugUtil.psiToString` of the `PsiJavaFile`.

| file | does |
|---|---|
| `JavaPsiOracle.java` | the oracle: a single-file `java` program on the jars of `jars.sha256` |
| `JavaLexOracle.java` | the lexer oracle: java-syntax's `JavaLexer` (and `JavaDocLexer` on each doc comment) over a list, one hash line per file as `ksdump --lex` prints it |
| `jars.sha256` | the 23 jars both run on, copied from the IDE to `~/.cache/psi-syntax/java-oracle/lib` and checked before every run, so an IDE update cannot change the oracle silently |

Their driver is `java-oracle`, a binary of `crates/ide-oracle` (the oracle harness, shared with accio's
`scripts/bench plugin-xml`; decision 2026-11-05-ide-oracles-run-from-rust in `../../../docs/decisions/`), here `J`:

```sh
J="cargo run --release -q -p ide-oracle --bin java-oracle --"
```

| command | does |
|---|---|
| `$J jars` | copies the pinned jars to the cache where missing, checks them and prints the directory (`-cp "$LIB/*"`); a jar of another IDE build is refused |
| `$J psi ARGS` | runs `JavaPsiOracle.java ARGS` with `-Xss1g -Xmx8g` on those jars |
| `$J lex [--lines OUT] ARGS` | runs `JavaLexOracle.java ARGS`, its lines to `OUT` (a failed write fails the run) or stdout |
| `$J vendor-goldens [--repo C] [--rev R] [--out testdata/java]` | writes `testdata/java/` from intellij-community at the revision (`git cat-file`, never the working tree; `parser-full/` and `parser-partial/` rewritten whole) |
| `$J check-goldens [OUT] [--run]` | compares an oracle run over `testdata/java/manifest.tsv` (`--node`; `--run` makes it into OUT, default `~/.cache/psi-syntax/oracle/java-goldens`) with the goldens, byte for byte, naming each differing file's first differing line |
| `$J unmarked CHECKOUT` | the tracked `.java` files of a checkout without IDE test markup, as psi-tools' `unmarked.py` does for Kotlin |
| `$J mutate OUT [--per-file 2] [--seed java-mut]` | one-place mutations of the PSI-clean goldens, after psi-tools' `mutate.js` |
| `$J lex-fuzz GOLDENS OUT [COUNT] [SEED]` | lexer fuzz inputs: the golden inputs with lexer-state characters and snippets inserted, replaced or deleted |

`--ide DIR` names the IDE (default `/Applications/Idea.app/Contents`); the oracles run on its JBR, `JAVA=...`
overrides it. `mutate` and `lex-fuzz` draw with Python's generator (`ide_oracle::random::PyRandom`), so they write the
corpora the Python scripts before them wrote, byte for byte.

## Running it

```sh
$J psi --list LIST --root ROOT --out OUT [--level JDK_28] [--trim] [--node]
```

It writes `OUT/<path>.psi.txt` (`DebugUtil.psiToString(file, true, false)`, trimmed, as
`ParsingTestFixture.checkResult` prints it), with `--node` also `OUT/<path>.node.txt`
(`DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true)`, trimmed: the `*_node.txt` goldens,
which IntelliJ also checks against the `SyntaxNode` tree of java-syntax alone), and
`OUT/index.jsonl`: `path`, `dump`, `level`, `entry`, `psiErrors`, `crlf`, `firstError` or `failure`.
`--serve` reads `LIST<TAB>ROOT<TAB>OUT` lines from stdin and prints `done` after each, in one JVM.

With `--getters` it writes `OUT/<path>.getters.txt` instead of the dumps: the answers of the PSI getters that
`java_syntax::psi` ports (`PsiField.getTypeElement()`, `PsiModifierList.hasModifierProperty(...)`,
`PsiJavaCodeReferenceElementImpl.getKindEnum(...)`, ...), one line `<id> <getter> <answer>` per getter of each node
by its element type, in the order and format of `crates/java-syntax/src/psi/getters.rs`. The id is the node's index
in a preorder walk of `file.getNode()`, psi-syntax's `NodeId` (the node dumps being equal); `!` is an answer
IntelliJ threw on (only an implicit class's `getImplementsList()` does, on the corpora). As in the IDE,
`LOG.error` logs instead of throwing (a factory of quiet `DefaultLogger`s) and Java asserts are off;
`PsiAugmentProvider`'s extension point is registered empty, as `JavaCoreApplicationEnvironment` does.
`ksdump --getters --check OUT` compares ours with them (an oracle `!` takes any answer of ours).

A list line is `PATH[<TAB>LEVEL[<TAB>ENTRY[<TAB>TRIM]]]` (more columns are ignored, so
`testdata/java/manifest.tsv` is a list):

- `LEVEL`: a `LanguageLevel` constant, set as the file's `LanguageLevelKey.FILE_LANGUAGE_LEVEL_KEY` as the
  tests do. Default `--level`, itself `LanguageLevel.HIGHEST` (`JDK_28`). The tests parse at
  `JavaTestUtil.getMaxRegisteredLanguageLevel()`, the last constant, `JDK_X`, unless a test calls
  `setLanguageLevel`; the manifest records the level of each.
- `ENTRY`: `file` (default), or one of the 12 parser entries of the parser-partial tests, run on a
  `PsiJavaFileImpl` whose root is `test.java.file` as `JavaParsingTestConfiguratorBase` does (the list
  is in the header of `JavaPsiOracle.java`).
- `TRIM`: `1` trims the text (chars `<= ' '`) as `ParsingTestFixture.loadFile` does, `0` does not;
  default `--trim`.

The text is decoded as strict UTF-8 (a malformed file is a `failure` without a dump) and its line
separators are converted to `\n`, as the IDE does when it loads a file.

## Measured (2026-11-01, Apple M5 Max, JDK 26.0.2)

- Goldens: 1036 of 1036 `.txt` and 1036 of 1036 `_node.txt` byte-equal (504 parser-full, 532
  parser-partial; `check-goldens`), with the IDE's full classpath and with the 23 pinned jars alike; on
  the IDE's JBR 25.0.4 too (2026-11-05), whose dumps of the lexer fuzz equal the cached ones.
  The level is honoured: parsed at `JDK_X` instead of their own level, 4 of the 7 tests that set one
  differ from their golden.
- intellij-community at `3caa0bf060` (`~/.cache/accio-bench/intellij-community`): 91 807 tracked
  `.java` files, 68 263 without markup (215 750 655 bytes), parsed at `JDK_28` in 55.5 s wall, 1.96 GB
  peak RSS; 3 are not UTF-8 (`failure`), 990 have a `PsiErrorElement`, none has a `\r`.
- Mutations (`mutate`, seed `java-mut`, 2 sites per operator and file): 624 PSI-clean goldens
  (412 with an error left out), 18 575 mutants, 19 199 files; 83 mutants of parser-partial fragments
  break their entry's precondition (`parseClassBodyWithBraces` requires `{`, the pattern test requires
  `PatternParser.isPattern`) and are dropped from the oracle list: 19 116 files, 10 162 with a
  `PsiErrorElement`, 5.4 s. Regenerating gives identical files.

## Caches (`~/.cache/psi-syntax`)

| path | holds |
|---|---|
| `java-oracle/lib/` | the pinned jars |
| `corpus/java-community-3caa0bf.list` | the community list (`unmarked`) |
| `oracle/java-community-3caa0bf/` | its dumps (`.psi.txt`, `.node.txt`, `index.jsonl`; 7.1 GB on disk) |
| `fuzz/java-mut/` | the mutants, `original/`, `origins.json` and the full `list` |
| `fuzz/java-mut.list` | the oracle list (without the 83 precondition failures) |
| `fuzz/java-mut.oracle/` | its dumps |
| `fuzz/java-lex/` | the lexer fuzz inputs and their `list` (`lex-fuzz`, 20 000 files, seed `java-lex`) |
| `fuzz/java-lex.oracle/` | their dumps (17 604 with a `PsiErrorElement`, 5.8 s) |
| `lex/java-community-3caa0bf.oracle.txt`, `lex/java-mut.oracle.txt`, `lex/java-lex.oracle.txt` | `JavaLexOracle.java`'s lines for the three corpora |
| `getters/2/java-goldens/`, `getters/2/java-community-3caa0bf/`, `getters/2/java-mut/`, `getters/2/java-lex/` | `--getters` of `testdata/java/manifest.tsv` and the three corpora (81.9 million lines, 1.8 GB; 70 s for the four at once). `2` is the line format: a change of the lines a getter prints writes a new number, so a tree before it keeps its own (`getters/` holds the first) |

Rebuild them:

```sh
S=$HOME/.cache/psi-syntax C=$HOME/.cache/accio-bench/intellij-community
$J unmarked $C > $S/corpus/java-community-3caa0bf.list
$J psi --list $S/corpus/java-community-3caa0bf.list --root $C --out $S/oracle/java-community-3caa0bf --node
$J mutate $S/fuzz/java-mut
$J psi --list $S/fuzz/java-mut/list --root $S/fuzz/java-mut --out $S/fuzz/java-mut.oracle --node   # then drop the `failure` lines from the list
$J lex-fuzz testdata/java $S/fuzz/java-lex 20000 java-lex
$J psi --list $S/fuzz/java-lex/list --root $S/fuzz/java-lex --out $S/fuzz/java-lex.oracle --node
```

The getter lines, which `scripts/gate.sh` compares `ksdump --getters` with (the same lists and roots as the dumps):

```sh
S=$HOME/.cache/psi-syntax C=$HOME/.cache/accio-bench/intellij-community
g() { $J psi --getters "$@"; }
g --list testdata/java/manifest.tsv --root testdata/java --out $S/getters/2/java-goldens
g --list $S/corpus/java-community-3caa0bf.list --root $C --out $S/getters/2/java-community-3caa0bf
g --list $S/fuzz/java-mut.list --root $S/fuzz/java-mut --out $S/getters/2/java-mut
g --list $S/fuzz/java-lex/list --root $S/fuzz/java-lex --out $S/getters/2/java-lex
```

The lexer oracle's cached lines, which `scripts/gate.sh` compares `ksdump --lex` with:

```sh
S=$HOME/.cache/psi-syntax C=$HOME/.cache/accio-bench/intellij-community
$J lex --lines $S/lex/java-community-3caa0bf.oracle.txt --list $S/corpus/java-community-3caa0bf.list --root $C
$J lex --lines $S/lex/java-mut.oracle.txt --list $S/fuzz/java-mut.list --root $S/fuzz/java-mut
$J lex-fuzz testdata/java $S/fuzz/java-lex 20000 java-lex
$J lex --lines $S/lex/java-lex.oracle.txt --list $S/fuzz/java-lex/list --root $S/fuzz/java-lex
```

Regenerated with these commands on 2026-11-05, `vendor-goldens` left `testdata/java/` unchanged, `unmarked`,
`mutate` and `lex-fuzz` wrote the cached list and corpora byte for byte, and `lex` the cached lines of the lexer
fuzz.
