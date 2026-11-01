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
| `fetch-jars.sh` | copies those 23 jars from `/Applications/Idea.app/Contents` to `~/.cache/psi-syntax/java-oracle/lib` and checks their sha256, so an IDE update cannot change the oracle silently |
| `vendor_goldens.py` | writes `testdata/java/` from intellij-community at the revision (`git cat-file`, never the working tree) |
| `check-goldens.py` | compares an oracle run over `testdata/java/manifest.tsv` with the goldens, byte for byte |
| `unmarked.py` | the tracked `.java` files of a checkout without IDE test markup, as psi-tools' `unmarked.py` does for Kotlin |
| `mutate.py` | one-place mutations of the PSI-clean goldens, after psi-tools' `mutate.js` |

## Running it

```sh
LIB=$(tools/java-oracle/fetch-jars.sh)
java -Xss1g -Xmx8g -cp "$LIB/*" tools/java-oracle/JavaPsiOracle.java \
  --list LIST --root ROOT --out OUT [--level JDK_28] [--trim] [--node]
```

It writes `OUT/<path>.psi.txt` (`DebugUtil.psiToString(file, true, false)`, trimmed, as
`ParsingTestFixture.checkResult` prints it), with `--node` also `OUT/<path>.node.txt`
(`DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true)`, trimmed: the `*_node.txt` goldens,
which IntelliJ also checks against the `SyntaxNode` tree of java-syntax alone), and
`OUT/index.jsonl`: `path`, `dump`, `level`, `entry`, `psiErrors`, `crlf`, `firstError` or `failure`.
`--serve` reads `LIST<TAB>ROOT<TAB>OUT` lines from stdin and prints `done` after each, in one JVM.

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
  parser-partial; `check-goldens.py`), with the IDE's full classpath and with the 23 pinned jars alike.
  The level is honoured: parsed at `JDK_X` instead of their own level, 4 of the 7 tests that set one
  differ from their golden.
- intellij-community at `3caa0bf060` (`~/.cache/accio-bench/intellij-community`): 91 807 tracked
  `.java` files, 68 263 without markup (215 750 655 bytes), parsed at `JDK_28` in 55.5 s wall, 1.96 GB
  peak RSS; 3 are not UTF-8 (`failure`), 990 have a `PsiErrorElement`, none has a `\r`.
- Mutations (`mutate.py`, seed `java-mut`, 2 sites per operator and file): 624 PSI-clean goldens
  (412 with an error left out), 18 575 mutants, 19 199 files; 83 mutants of parser-partial fragments
  break their entry's precondition (`parseClassBodyWithBraces` requires `{`, the pattern test requires
  `PatternParser.isPattern`) and are dropped from the oracle list: 19 116 files, 10 162 with a
  `PsiErrorElement`, 5.4 s. Regenerating gives identical files.

## Caches (`~/.cache/psi-syntax`)

| path | holds |
|---|---|
| `java-oracle/lib/` | the pinned jars |
| `corpus/java-community-3caa0bf.list` | the community list (`unmarked.py`) |
| `oracle/java-community-3caa0bf/` | its dumps (`.psi.txt`, `.node.txt`, `index.jsonl`; 7.1 GB on disk) |
| `fuzz/java-mut/` | the mutants, `original/`, `origins.json` and the full `list` |
| `fuzz/java-mut.list` | the oracle list (without the 83 precondition failures) |
| `fuzz/java-mut.oracle/` | its dumps |

Rebuild them:

```sh
S=$HOME/.cache/psi-syntax C=$HOME/.cache/accio-bench/intellij-community LIB=$(tools/java-oracle/fetch-jars.sh)
tools/java-oracle/unmarked.py $C > $S/corpus/java-community-3caa0bf.list
java -Xss1g -Xmx8g -cp "$LIB/*" tools/java-oracle/JavaPsiOracle.java --list $S/corpus/java-community-3caa0bf.list \
  --root $C --out $S/oracle/java-community-3caa0bf --node
tools/java-oracle/mutate.py $S/fuzz/java-mut
java -Xss1g -Xmx8g -cp "$LIB/*" tools/java-oracle/JavaPsiOracle.java --list $S/fuzz/java-mut/list \
  --root $S/fuzz/java-mut --out $S/fuzz/java-mut.oracle --node   # then drop the `failure` lines from the list
```
