# C# parsing goldens

The inputs of Roslyn's own parser and lexer tests, `src/Compilers/CSharp/Test/Syntax/Parsing/*.cs` and
`src/Compilers/CSharp/Test/Syntax/LexicalAndXml/*.cs` of dotnet/roslyn at
`35d9211b841e7613c1d2f8f5af6d628ace696c4c` (the commit `Microsoft.CodeAnalysis.CSharp` 5.9.0 is built from; MIT, see
NOTICE), each with the C# oracle's dump of it. `tools/csharp-oracle/vendor-goldens.sh` wrote them: it parses the
test sources with Roslyn itself and takes every call that parses a constant text, with the options and the entry
point the test parses it with (`tools/csharp-oracle/README.md`, "Goldens", says which calls and what is left out).

| file | is |
|---|---|
| `TestClass/Member[_N].cs` | the text the test parses, as UTF-8 (no trimming, no line-ending change) |
| `TestClass/Member[_N].tree.txt` | the oracle's dump of it (`tools/csharp-oracle/README.md`, "The dump") |
| `manifest.tsv` | `NAME<TAB>VERSION<TAB>ENTRY<TAB>DOC<TAB>SOURCE`: the language version (`latest`, `preview` or a number such as `7.3`), the entry (`compilation-unit`, `expression`, `statement`, `member`, `type-name`, `name`, `attribute-argument-list`, `tokens`), the documentation mode (`parse`, `diagnose`, `none`) and the test call's file and line |

The dumps are the oracle's, not the tests' expectations (Roslyn's tests check a prefix walk of kinds, not the
whole tree): a golden is what Roslyn 5.9.0 prints for that input. Never edit one; regenerate:

```sh
tools/csharp-oracle/vendor-goldens.sh            # reads ~/.cache/psi-syntax/roslyn at the pin
python3 tools/csharp-oracle/check_offsets.py testdata/csharp/manifest.tsv testdata/csharp testdata/csharp
```

A rerun over the same pin writes identical files, at any `--threads`.
