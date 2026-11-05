# Java parsing goldens

IntelliJ's Java parsing goldens from `java/java-tests/testData/psi/parser-{full,partial}` of
JetBrains/intellij-community at `decc943674edfe7ecc620d2d60b61be38f876994`, written by
`java-oracle vendor-goldens` (`crates/ide-oracle`; Apache 2.0, as intellij-community). One entry per test method of
`java/java-frontback-tests/testSrc/com/intellij/java/parser/**/AbstractBasic*Test.java`
that compares a dump: 504 parser-full, 532 parser-partial.

| file | is |
|---|---|
| `NAME.java` | the parsed text: the test data file (parser-full, verbatim; the test trims it) or the string the test passes (parser-partial, exact) |
| `NAME.txt` | `DebugUtil.psiToString(file, true, false)`, trimmed, as committed |
| `NAME_node.txt` | `DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true)`, trimmed, as committed |
| `manifest.tsv` | `PATH<TAB>LEVEL<TAB>ENTRY<TAB>TRIM<TAB>TEST`: the level the test parses at (`JDK_X`, the last `LanguageLevel`, unless it calls `setLanguageLevel`: 7 do), the parser entry (`file` or one of `JavaPsiOracle.java`'s), whether the test trims the text, and the test method |

The Java PSI oracle (`tools/java-oracle`) reproduces all 1036 `.txt` and all 1036 `_node.txt` byte
for byte when run over `manifest.tsv` with `--node` (`java-oracle check-goldens --run`).

Not vendored: `parser-full/jshell` (JShell, another parser definition), `LongClass` (its test calls
`doTest(false)`, so the golden is not compared), `BigIf` (its test counts nodes), and the data files
no test reads: `expressionParsing/Parenth2.java`, `parser-partial/expressions/InstanceOf2.txt`.
