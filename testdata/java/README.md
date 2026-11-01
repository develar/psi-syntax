# Java parsing goldens

IntelliJ's Java parsing goldens from `community/java/java-tests/testData/psi/parser-{full,partial}` of
the IntelliJ monorepo at `3d034e2526507767a19528888066e47ad2898bb1`, written by
`tools/java-oracle/vendor_goldens.py` (Apache 2.0, as the monorepo). One entry per test method of
`community/java/java-frontback-tests/testSrc/com/intellij/java/parser/**/AbstractBasic*Test.java`
that compares a dump: 504 parser-full, 532 parser-partial.

| file | is |
|---|---|
| `NAME.java` | the parsed text: the test data file (parser-full, verbatim; the test trims it) or the string the test passes (parser-partial, exact) |
| `NAME.txt` | `DebugUtil.psiToString(file, true, false)`, trimmed, as committed |
| `NAME_node.txt` | `DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true)`, trimmed, as committed |
| `manifest.tsv` | `PATH<TAB>LEVEL<TAB>ENTRY<TAB>TRIM<TAB>TEST`: the level the test parses at (`JDK_X`, the last `LanguageLevel`, unless it calls `setLanguageLevel`: 7 do), the parser entry (`file` or one of `JavaPsiOracle.java`'s), whether the test trims the text, and the test method |

The Java PSI oracle (`tools/java-oracle`) reproduces all 1036 `.txt` and all 1036 `_node.txt` byte
for byte when run over `manifest.tsv` with `--node` (`tools/java-oracle/check-goldens.py`).

Not vendored: `parser-full/jshell` (JShell, another parser definition), `LongClass` (its test calls
`doTest(false)`, so the golden is not compared), `BigIf` (its test counts nodes), and the data files
no test reads: `expressionParsing/Parenth2.java`, `parser-partial/expressions/InstanceOf2.txt`.
