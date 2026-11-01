# kotlin-syntax: what an agent must know

A lossless Kotlin lexer and parser in Rust, ported from the Kotlin compiler's own. The goal is exact: for every input,
`dump::psi_to_string` prints what Kotlin 2.4.21's `DebugUtil.psiToString(file, true, false).trim()` prints, byte for
byte. Consumers: accio (code search; declarations, call sites, imports) and Branch Deck's `code-symbols` (diff
highlighting, navigation, file summaries). Both read the tree directly: there is no tree-sitter compatibility layer and
there must never be one.

## Sources to port (read them, port them faithfully)

Kotlin at 9180ce6a7a7d3627f88b424717583f28ddca052d (2.4.21), a sparse checkout at
`K=/Users/develar/.branch-deck/pool/tree-sitter-kotlin-8608ba7a/1/tree-sitter-kotlin/local/kotlin`. A file absent on
disk is read with `git -C $K show HEAD:<path>`.

| what | source |
|---|---|
| Tokens | `compiler/psi/psi-api/src/org/jetbrains/kotlin/lexer/KtTokens.java`, `KtKeywordToken.java`, `KtSingleValueToken`, `KtModifierKeywordToken` |
| Lexer | `compiler/psi/parser/src/org/jetbrains/kotlin/lexer/Kotlin.flex` (the JFlex spec; `_JetLexer.java` is its generated form), `KotlinLexer.java` |
| Element types | `compiler/psi/psi-api/src/org/jetbrains/kotlin/KtNodeTypes.java` (+ `KtStubElementTypes` names where the dump prints them) |
| Builder | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/SemanticWhitespaceAwarePsiBuilder*.java`, `TruncatedSemanticWhitespaceAwarePsiBuilder.java`, and IntelliJ's `PsiBuilderImpl` (`/Users/develar/.cache/accio-bench/intellij-community/platform/core-impl/src/com/intellij/lang/impl/PsiBuilderImpl.java`): markers, `precede`, `done`, `drop`, `rollbackTo`, `collapse`, `error`, `doneBefore`, `errorBefore`, edge (whitespace/comment) binders, `remapCurrentToken`, lookahead |
| Comment binding | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/KotlinWhitespaceAndCommentsBinders.kt`, plus the platform's default binders (`WhitespacesBinders`) |
| Parser | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/AbstractKotlinParsing.java`, `KotlinParsing.java`, `KotlinExpressionParsing.java`, `KotlinParser.java`, the token-stream patterns (`FirstBefore`, `LastBefore`, `AbstractTokenStreamPattern`, …), `ParseUtils.kt` |
| KDoc | `compiler/psi/parser/src/org/jetbrains/kotlin/kdoc/lexer/KDoc.flex`, `KDocLexer.java`, `kdoc/parser/KDocParser.java`, `KDocLinkParser.kt`, `compiler/psi/psi-api/src/org/jetbrains/kotlin/kdoc/lexer/KDocTokens.java`, `kdoc/parser/KDocElementTypes.java`: the dump expands a doc comment's inner tree |

Keep the Java structure: one Rust function per Java method, the same name in snake_case, the same order, so a diff
against the Java reads line by line. Port behaviour, not just shape: newline sensitivity, rollback, recovery sets,
error messages (the dump prints `PsiErrorElement:<message>`) and the exact element types.

## Layout

| module | holds |
|---|---|
| `src/tokens.rs` | `TokenKind` (every Kotlin and KDoc token, plus WHITE_SPACE, BAD_CHARACTER), its debug name as the dump prints it, the keyword sets (hard, soft, modifier) |
| `src/unicode.rs` | `[:letter:]`, `[:digit:]` (Kotlin.flex) and `[:jletter:]` (KDoc.flex) above ASCII, decoded from the character maps of `_JetLexer.java` / `_KDocLexer.java`; generated, do not edit |
| `src/lexer.rs`, `src/kdoc/lexer.rs` | the ports of Kotlin.flex and KDoc.flex (states, string templates, raw strings, `$$` prefixes, nested block comments, shebang) |
| `src/kdoc/parser.rs` | `KDocParser` and `KDocLinkParser`: the tree runs them on every doc comment and markdown link |
| `src/node_kind.rs` | `NodeKind`, the composite element types with their dump names (`FUN`, `CLASS`, `KtFile` root, `PsiErrorElement`) |
| `src/builder.rs` | the PsiBuilder port: token cursor that skips whitespace and comments, markers with rollback, the newline mode stack, edge binders; produces the tree |
| `src/tree.rs` | the lossless tree: one arena (`Vec`) of nodes, `u32` ids, every node a kind and a byte range, children in order, tokens as leaves; whitespace and comments are leaves where PSI puts them. The public API consumers read: `Tree::parse(text, FileKind::{Kt,Kts})`, `root()`, `kind()`, `range()`, `children()`, `parent()`, `text()`, `first_child_of_kind()`, an iterator over tokens. `Send + Sync`, no lifetimes in the stored tree |
| `src/parsing/*.rs` | `abstract_parsing.rs`, `kotlin_parsing.rs`, `expression_parsing.rs`, `patterns.rs`: the parser ports; `kotlin_parser.rs`: the `KotlinParser` entry points |
| `src/dump.rs` | `psi_to_string(&Tree, file_name) -> String`: the exact PSI debug format |
| `src/bin/ksdump.rs` | `ksdump --list LIST --root ROOT --out OUT`: writes `OUT/<path>.psi.txt` per file like the oracle, and `ksdump --bench --list LIST --root ROOT`: parse-only throughput in MB/s |
| `tests/goldens.rs` | every pair under `testdata/{psi,spec}`: parse the `.kt`/`.kts`, compare `psi_to_string` with the `.txt` (trailing whitespace trimmed); prints the pass count and the first differing line of each failure; `KS_STRICT=1` fails on any difference but the stale pair of `tests/common::KNOWN_STALE` |
| `tests/oracle.rs` | each `tests/oracle/NAME.kt` against the live PSI oracle's dump `NAME.txt` of it: inputs that once differed |
| `tests/lexer.rs`, `tests/kdoc.rs`, `tests/robustness.rs` | the goldens' leaf texts against our tokens; the goldens' `KDoc` subtrees against ours; arbitrary bytes parse losslessly without a panic |

## The dump format (`DebugUtil.psiToString(file, showWhitespaces = true, showRanges = false)`)

First line `KtFile: <file name>`, then each node indented two spaces per depth: a composite prints its element type
(`PROPERTY`), an empty composite gets a child line `<empty list>`, a leaf prints `PsiElement(<token debug name>)('<text>')`,
whitespace `PsiWhiteSpace('<text>')`, a comment `PsiComment(EOL_COMMENT)('<text>')`, an error `PsiErrorElement:<message>`.
Texts escape `\n`, `\t`, `\r` and `\\`, `'` as `DebugUtil` does. The whole dump is trimmed.

## Oracles

1. `cargo test --release --test goldens`: the 789 vendored goldens of the compiler's own parsing tests (Apache-2.0,
   see NOTICE). The first gate; run it after every change.
2. The live PSI oracle (kotlin-compiler-embeddable 2.4.21) on real code:
   `java -Xss512m -Xmx8g -cp "$HOME/.cache/kotlin-syntax/psi-tools/tools/psi-oracle/lib/*" $HOME/.cache/kotlin-syntax/psi-tools/tools/psi-oracle/PsiOracle.java --list LIST --root ROOT --out OUT`
   writes `OUT/<path>.psi.txt` and `OUT/index.jsonl` (`path`, `psiErrors`, `firstError`). Ready dumps:
   `$HOME/.cache/kotlin-syntax/oracle/community-3caa0bf/` for the list `$HOME/.cache/kotlin-syntax/corpus/community-3caa0bf.list`
   over `/Users/develar/.cache/accio-bench/intellij-community` (55 501 files). Compare with
   `scripts/compare.py ORACLE_DIR OURS_DIR`.
3. The lexer oracle: `java -cp "$HOME/.cache/kotlin-syntax/psi-tools/tools/psi-oracle/lib/*" scripts/LexOracle.java --list LIST
   --root ROOT` prints a hash of each file's tokens (Kotlin, and KDoc inside each doc comment) as `ksdump --lex` does;
   `diff` the `cut -f1,2` of both. `scripts/lex-fuzz.py` makes mutated inputs for it.
4. Error recovery: `scripts/parse-fuzz.py LIST ROOT OUT [SEED]` mutates files at token boundaries (punctuation,
   keywords, modifiers) for the PSI oracle and `scripts/compare.py`; `scripts/lex-fuzz.py`'s inputs serve it too.
   `scripts/psi-reduce.py FILE OUT` shrinks a differing file to a small input; a fixed case goes to `tests/oracle/`
   (`NAME.kt` and the oracle's dump `NAME.txt`), which `cargo test --release --test oracle` compares.

Never edit a golden or an oracle dump to make a test pass. A difference is a bug in the port until the Java says
otherwise.

## Porting conventions (the builder and parser API)

- `AbstractKotlinParsing` is a trait implemented by `KotlinParsing<'b>` and `KotlinExpressionParsing<'p, 'b>`; its
  helpers are provided methods (`self.at(LBRACE)`, `self.expect_with(RPAR, "Expecting ')'")`). `myBuilder` is
  `self.my_builder()`, a `SemanticWhitespaceAwarePsiBuilder` (the Java builder seen through its adapters).
- `KotlinExpressionParsing` is a borrow of its `KotlinParsing`: `myExpressionParsing.f()` is
  `self.my_expression_parsing().f()`, `myKotlinParsing.g()` is `self.my_kotlin_parsing.g()`.
- A marker is a `Copy` handle whose methods take the builder: `marker.done(self.my_builder(), FUN)`,
  `marker.precede(self.my_builder())`, `marker.error(self.my_builder(), "...")`, `rollback_to`, `drop`, `collapse`,
  `done_before`, `error_before`, `set_custom_edge_token_binders(b, Some(left), None)`.
- A token type is `Option<TokenKind>` (`null` = `None`) and compares with a token directly: `self.tt() == LBRACE`. A
  `TokenSet` is a `const`, `TokenSet::create(&[..])`, `or_set`, `and_set`, `and_not`; `contains` takes a token, a node
  type or an `Option`. Composite types are `NodeKind`; an `IElementType` result is `Option<NodeKind>`.
- Overloads get suffixed names: the fewest-argument form keeps the name, the others `_with` (or a descriptive suffix,
  e.g. `expect_with_recovery`, `parse_type_ref_with_intersections`), documented on each function.
- Every stub says `TODO(port): <File>.java:<line>`; port the method there, keep the order.

## Rules

- Dependencies: a well-maintained crate is better than a reinvented one when it is measurably as fast or faster
  and keeps the output byte-identical; each one is justified in its commit (what it replaces, the measured effect,
  its maintenance). Nothing that needs a C toolchain or pulls a large tree. No `unsafe` of our own, no `unwrap` on
  input-dependent paths: any byte sequence parses (invalid UTF-8 is lexed as BAD_CHARACTER bytes) and never panics.
- Speed is a goal: no allocation per token beyond the arena, no `String` per leaf (texts are ranges), no recursion
  depth that a 4 MB file of nested expressions can overflow without a guard (PSI's own limits apply).
- `scripts/gate.sh` prints PASS before every commit: clippy, `KS_STRICT=1 cargo test --release`, byte equality of
  all 55 501 community dumps and of the cached fuzz corpora with the PSI oracle. A faster change that fails it is
  not a change.
- The default branch is `master`. Commit messages: imperative subject ≤ 72 characters, a body that says what the
  change does, its measured effect and that the gate passed.
