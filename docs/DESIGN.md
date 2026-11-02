# psi-syntax: design

AGENTS.md says what to port and how to check it; this document says how the port is built, why, what it costs and what
is still open. The design sections are in the present tense. Every number comes with the commit it was measured on
and the date.

## Contract

For every byte sequence, `dump::psi_to_string(&tree, name)` prints exactly what Kotlin 2.4.21's
`DebugUtil.psiToString(file, true, false).trim()` prints, and the parse never panics. `scripts/gate.sh` checks this
before every commit:

- clippy, on default features and with `counters,alloc-stats`;
- `KS_STRICT=1 cargo test --release`;
- byte equality with the PSI oracle on all 55 501 community files and on the cached parse-fuzz and lex-fuzz corpora;
- `ksdump --check-bodies`: each of the 229 960 community bodies parsed on its own equals the full parse's subtree;
- the same with nesting limit 3 (`--nesting-limit 3`) on the community files and the mutations, so the too-deep paths
  run on ordinary code: every tree lossless, every body exact.

For Java the contract is the same against IntelliJ 271's Java PSI: `java_syntax::dump::psi_to_string` prints
`DebugUtil.psiToString(file, true, false).trim()` and `node_tree_to_string` the `_node.txt` dump. The gate checks both
dumps on the 1036 goldens (`KS_STRICT=1`), the 68 260 UTF-8 community `.java` files, the 19 116 golden mutations and
the 20 000 lexer fuzz inputs, and the Java lexers' token lines on the same corpora (see "Java", "Oracle"); then
`ksdump --check-bodies` on the community `.java` files (443 161 code blocks), and with `--nesting-limit 3` on them and
on the golden mutations.

Speed work therefore changes only three things, none of which changes a parser decision:

- how the cursor and the markers are represented;
- when and what is allocated;
- the layout of the finished tree.

Before and after each such change, the 25 `--features counters` counters (cursor calls, markers, rollbacks, lexed,
reused and skipped tokens) are compared. The token cache was meant to move only the raw cursor-call counts, and the
brace skip only the calls, markers and tokens skipped inside lazy blocks. Every other change left all 25 identical.

## Workspace

| crate | holds | depends on |
|---|---|---|
| `psi-builder` | `PsiBuilderImpl<L>`, `Marker`, the `WhitespacesAndCommentsBinder` contract, `Tree<L>` and `Parser<L>` (the driver), `TreeData`, the dump's layout, `ElementTypeSet`, `Lexed<T>`, `char_at`, `in_ranges`, the builder's counters | nothing |
| `kotlin-syntax` | `TokenKind`, `NodeKind`, `TokenSet`, the Kotlin and KDoc lexers, `Binder`, `SemanticWhitespaceAwarePsiBuilder*`, the parsers, KDoc, `Kotlin: Language` | `psi-builder` |
| `java-syntax` | `TokenKind`, `NodeKind`, `TokenSet`, the Java and JavaDoc lexers, `Binder`, `SyntaxTreeBuilder` (the Java parsers' builder handle), `Java: Language`, the dumps, the parsers | `psi-builder` |
| `ksdump` | the binary | `kotlin-syntax`, `java-syntax` |

`psi-builder` is generic over `L: Language`, a marker type whose associated items are what IntelliJ reads from a
`ParserDefinition` and from element types: `Token`, `Node` (each with a dense index below 256 and a debug name),
`Binder`, `Lazy` (what a chameleon is reparsed with), `FileKind`, `Frame` (a builder with the language layers' state over it, `frame_builder`),
`FILE`, `ERROR_ELEMENT`, `is_whitespace_or_comment`, `is_left_bound`, `is_chameleon_token`, `chameleon`, `is_body`,
`joined_token_type` (the cursor cache's derived token), `bracket_depth_change` (the too-deep skip), `tokenize_into`,
`parse_file` and `parse_chameleon`. `tokenize_into` gets the file's `FileKind` and `Chameleon::file_kind` gives it to
`parse_chameleon`, because Java's lexer and parsers depend on the file's language level. The driver hands `parse_chameleon` a `Chameleon`: the chameleon's text and three
ways to a builder (the body's file tokens when they lex alone the same, else relexed; relexed; the caller's own tokens).
`kotlin_syntax::{Tree, Parser, PsiBuilderImpl, ElementType}` are aliases of the generic types for `Kotlin`, so the
Kotlin parsers and consumers name them as before.

Everything is monomorphized per language; nothing is `dyn`. A function the Kotlin crate's own code used to inline may
be instantiated differently from another crate: `skip_whitespace`, inlined before, became a call on every cursor move
after the split and cost 0.8 %, so it is `#[inline(always)]`; Kotlin's `getJoinedTokenType`, a call before, is again
not forced inline. The split's measurements are under "Measurements".

## Pipeline

```
text ─ Language::tokenize_into ─> Lexed {types, starts}   (file tokens, Parser-owned buffers)
     ─ PsiBuilderImpl + KotlinParsing ─> production        (markers, one Rust fn per Java method)
     ─ begin_bind: balance_white_spaces, bind ─> TreeData   (Parser-owned columns, preorder)
         └─ at each chameleon (BLOCK, LAMBDA, KDoc, markdown link): suspend the frame, parse it, bind, resume
     ─ copy out ─> Tree {Arc<[u8]>, kinds, ranges, links, errors}   (exactly sized, Send + Sync)
```

### Lexer (`kotlin-syntax`: `src/lexer.rs`, `src/kdoc/lexer.rs`)

The lexers are hand ports of `Kotlin.flex` and `KDoc.flex`, not generated code:

- the same states and state stack (with `lBraceCount` and the interpolation prefix);
- longest match, and the earlier rule on a tie;
- `yypushback` where the spec uses it.

Offsets are bytes. A byte that does not start valid UTF-8 counts as one character. Character classes above ASCII come
from tables decoded out of the generated Java lexers' character maps (`src/unicode.rs`), so `[:letter:]` is Java's
own. ASCII is handled by byte tables and byte `match`es:

- `hard_keyword` and `TokenKind::soft_keyword_by_text` / `keyword_by_debug_name` are byte matches;
- `plain_identifier_end` scans a `const ASCII_IDENTIFIER_PART: [bool; 128]` table.

Lexing takes under 10 % of a parse, so it is optimized only where that is cheap and safe.

### Builder (`psi-builder` `src/builder.rs`, Kotlin's layer in `kotlin-syntax` `src/builder.rs`)

`PsiBuilderImpl` is ported with IntelliJ's semantics:

- the `MarkerProduction` order;
- rollback to a marker's production entry, freeing markers from the last entry back;
- the global `lastErrorIndex` error dedup;
- the edge binders, `remapCurrentToken` and `tokens_to_restore_on_rollback`.

Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl` adds the newline-mode and token-join stacks on top. The adapters
(`TruncatedSemanticWhitespaceAwarePsiBuilder`, `ForByClause`) are a stack inside the builder, seen through a handle,
`SemanticWhitespaceAwarePsiBuilder`.

What the port adds to the hot path:

- **A current-token cache** in `PsiBuilderImpl`, computed at every move rather than at the first read: `sig` (where
  `eof()` puts the cursor: past whitespace and comments after a move, the cursor itself after `rollbackTo`, which sets
  `myTokenTypeChecked`), `tt_raw` and `tt_joined` (`Language::joined_token_type`, Kotlin's `getJoinedTokenType` body).
  - `eof()`, `getTokenType()` and `current_token` (the layer's type: joined while Kotlin joins complex tokens, told
    on every push and pop of that stack, `set_joined`) are a store of `sig` into the cursor and a read, without a branch;
    the cursor itself moves over whitespace only where the Java's does (`eof()`, `mark()`), so `rawTokenIndex`,
    `rawLookup` and the lexemes markers record are the Java's.
  - It is recomputed wherever the cursor moves or `lex_types` changes: `advance_lexer`, `raw_advance_lexer` (and
    `advance_to_eof`), `rollback_to` (which also restores remapped tokens), `remap_current_token`, `remap_tokens`,
    `skip_balanced_block`, and `mark()` when it steps over whitespace `eof()` would not (after a rollback).
  - `myTokenTypeChecked` is kept as `checked` only for a remap: a token at the cursor remapped into whitespace or a
    comment is skipped only while it is false. `current_token` does not record it (a store on the hottest path), so
    a layer reading the cursor through it never remaps into whitespace (Kotlin remaps to soft keywords and
    identifiers only).
  - The cache lives in `PsiBuilderImpl`, not in the semantic-whitespace layer, because `Marker` methods mutate
    `PsiBuilderImpl` directly.
- **The semantic layer's stack tops** (`join_top`, `newlines_top`) are fields updated on push and pop, so
  `getTokenType()` reads a bool, not `Vec::last`; `newlineBeforeCurrentToken` walks the raw tokens before the cursor
  as a slice.
- **The handle caches** `top_trunc` and `min_eof` (the smallest non-negative `eof_position`). It sets them in
  `handle()`, `reborrow()` and `wrapped()`. This is sound by the borrow checker: `wrapped(self)` consumes the handle
  and `reborrow` freezes its parent, so the adapter prefix cannot change while the handle is alive.
- **Token text is compared as bytes** (`get_token_bytes`, which returns `b"?:"` and `b"?."` for joined tokens) in
  `at`, `at_set`, `expect` and the keyword helpers. Only a test calls `get_token_text`, so `str::from_utf8` is off the
  hot path.
- **`#[inline(always)]` on the cursor chain** (`tt`, `_at`, `token_matches`, `_at_set`, `at`, the handle's and the
  impl's `get_token_type`/`eof`).
- **`skip_balanced_block(open, close)`.** When no `Truncated` adapter is active, `advance_balanced_block` counts
  `LBRACE`/`RBRACE` over the raw `lex_types`, instead of the Java loop that joins `?.`/`?:`/`!!` and creates their collapse markers.
  The result is the same:
  - Both callers collapse the enclosing marker: `parse_block_with` collapses `BLOCK` and the lazy lambda collapses
    `LAMBDA_EXPRESSION`.
  - `bind` skips a collapsed marker's children.
  - The markers it leaves out are monotone in lexeme, so they move no `balance_white_spaces` edge.

  Under a truncation the Java loop runs unchanged.

Marker storage:

- **`MarkerData`** is 20 bytes. It holds kind, collapsed, the binders, element type, lexeme, done lexeme and the
  nesting where it was collapsed. An error message lives in a side table and is addressed by a `u32`. There are no
  light-tree links.
- **The production is a doubly linked list while parsing**, not Java's `IntArrayList`. `links[node]` is
  `[prev, next]`; node `2·id` is marker `id`'s start entry and `2·id + 1` its done entry, so `indexOf` is the marker's
  own node and `precede`, `doneBefore`, `errorBefore`, `drop` and `rollbackTo` touch only the entries they add or
  remove. Java inserts into and removes from the middle of its array (`ArrayList.add(index, …)`), and finds an entry
  by a linear window, a binary search by lexeme and a walk over the markers at that lexeme; a chain `a+a+…` preceded
  the same far-back marker at every operator, quadratic: 4.67 s for 100 000 operators on c796e73. The order of entries,
  and so the tree, is the array's. A marker done twice (Java only logs it) gets an extra done node; while one is
  linked, `drop` finds a marker's last done entry by walking back, as `lastIndexOf` does.
- **`entries`** is the list made into one array by `prepare_light_tree` and balanced in the same walk
  (`flatten_and_balance`): each entry's id, its balanced lexeme and, for a start entry, its element type and whether
  it is collapsed, so `bind` reads one contiguous array and no marker on its common path. The walk reads each lexeme
  from its marker before balancing writes it, as the Java's array does, because while no extra done node is linked
  each lexeme field belongs to one entry; with one linked, the array is made first and balanced after. The default
  binders take a shortcut exact by their contract: `DEFAULT_LEFT_BINDER`'s edge is the whitespace's end and
  `DEFAULT_RIGHT_BINDER`'s its start, whichever of the Java's three cases (a token view of positive, negative or
  zero size) applies, so neither builds a view.
- **`bind` walks the production directly**, with an explicit stack and no `prepareLightTree` links, in one loop over
  the entries (leaves, then open or close a composite or append an error), leaving its state machine only at a
  chameleon leaf, a collapsed marker and the end. It reproduces `prepareLightTree`'s rules:
  - any done closes the current marker at its own done lexeme;
  - the stack starts as `[root]`, so entries after the root's done become root children;
  - error items inside collapsed markers count for the global dedup, although their entries are skipped;
  - open markers close innermost first.

### Java (`java-syntax`)

The sources are java-syntax and platform/syntax of JetBrains/intellij-community at decc943 (AGENTS.md, "Java").

- **Lexers.** `lexer.rs` is `JavaLexer.kt` (its own dispatch on the located character: whitespace, `{`/`}` with the
  template brace counts on its state stack, comments, character and string literals, text blocks, templates, `#!`,
  `\u001a`; `locateCharAt` decodes unicode escapes, `mySymbolLength` their length) over `FlexJavaLexer`, the rules of
  `_JavaLexer.flex` matched as JFlex matches them (longest match, the earlier rule on a tie: the numeric rules are
  computed alternative by alternative). `doc_lexer.rs` is `JavaDocLexer.kt`: the merging adapter over
  `AsteriskStripperLexer` over `FlexJavaDocLexer`, each of `_JavaDocLexer.flex`'s 14 states a list of its rules in
  spec order. Offsets are bytes; the code compares characters only with ASCII ones, so boundaries are those of the
  UTF-16 original. `[:jletter:]` and `[:jletterdigit:]` above ASCII are decoded from the generated lexers' DFA tables by
  running them (`tools/java-unicode.py`), JFlex's `.` excludes `\n\r\u000B\u000C\u0085\u2028\u2029` (read off the
  same tables).
- **Builder.** The Java parsers run on platform/syntax's `SyntaxTreeBuilder`; the port runs them on `psi_builder`'s
  `PsiBuilderImpl` seen through `SyntaxTreeBuilder<'b>`, a handle with the `stopAt` offset of
  `JavaParserUtil.stoppingBuilder` (nested ones stop at the nearest), so no adapter is a type of its own and
  `DeclarationParser.parse` recursing on a stopping builder does not instantiate generics without end.
  `enforceCommentTokens({DOC_COMMENT_LEADING_ASTERISKS})` is folded into `Java::is_whitespace_or_comment`: that token
  occurs in no other builder and the Java comments in no doc comment's. `lastDoneMarker` and the marker accessors
  (`getNodeType`, `getStart/EndOffset`, `getStart/EndTokenIndex`) are generic `psi_builder` methods.
- **Lazy elements.** `CODE_BLOCK` is a body (collapsed by `parseBlockLazy`, reparsed by `parseCodeBlockDeep` at the
  file's level); `DOC_COMMENT` and `DOC_MARKDOWN_COMMENT` (Java lexer tokens) are lexed with `JavaDocLexer` and parsed by
  `JavaDocParser`; `DOC_REFERENCE_HOLDER` and `DOC_TYPE_HOLDER` (JavaDocParser remaps or collapses into them) are lexed
  with `JavaLexer(JDK_1_3)` (the type's through `JavaTypeEscapeLexer`). The four token-typed ones are `TokenKind`s, as
  Kotlin's KDoc is.
- **Files and entries.** `FileKind { level, entry }`: a file (`java.FILE`) or one of the 13 parser entries of the
  parser-partial goldens under `test.java.file` with "Unparsed tokens" over what is left, as
  `JavaParsingTestConfiguratorBase` runs them.
- **Dumps.** `node_tree_to_string` prints the element types (`DOC_SPACE` as PSI's `WHITE_SPACE`); `psi_to_string` the
  PSI class of each node and its `toString()` (`NodeKind::label`, from the goldens' 1036 dumps). Where a `toString()`
  computes its text, the dump ports that computation: `PsiPackageStatementImpl.getPackageName()` is
  `JavaSourceUtil.getReferenceText` (`push_reference_text`), the module name
  `PsiJavaModuleReferenceElementImpl.getReferenceText()`, and a `PsiInlineDocTag` without a name prints `null`.
- **Oracle.** `tools/java-oracle/JavaPsiOracle.java` runs IntelliJ 271's real Java PSI (java-syntax converted to PSI by
  `PsiSyntaxBuilderFactoryImpl`, as the IDE does) from 23 IDE jars pinned by sha256 (`fetch-jars.sh`), on a
  `CoreApplicationEnvironment` with the parsing part of `JavaCoreApplicationEnvironment`; a list line carries the
  level (set as `FILE_LANGUAGE_LEVEL_KEY`, as the tests do), the entry and the trim of each file. It reproduces all
  1036 vendored goldens of IntelliJ's Java parsing tests byte for byte, both dumps, which is what makes its dumps of
  other corpora the reference. `JavaLexOracle.java` prints `JavaLexer`'s and `JavaDocLexer`'s tokens as
  `ksdump --lex` does. Its cached dumps are listed in `tools/java-oracle/README.md`.
- **Bench.** `ksdump --bench` takes a list of Java files (`PATH[<TAB>LEVEL]`, every one a file), on default thread
  stacks; `--check-bodies` takes one as `--out` does (`PATH[<TAB>LEVEL<TAB>ENTRY<TAB>TRIM]`).
- **`JavaParserUtil.stoppingBuilder(builder, condition)`** (JavaParserUtil.kt:133) is the one method not ported.
  Nothing in intellij-community at decc943 calls it (the parsers call only `stoppingBuilder(builder, stopAt)`, from
  `braceMatchingBuilder` and `DeclarationParser.parse`), and its adapter would call the condition on every
  `tokenType` and `eof()` of every builder, where `SyntaxTreeBuilder` checks one offset. It was a `todo!` stub, a
  panic path in a crate that must never panic, and is removed; `java_parser_util.rs` says where it would go.

### Parser driver and chameleons (`psi-builder` `src/tree.rs`, `kotlin-syntax` `src/language.rs`)

`Parser` owns every buffer a parse needs:

- the frames (`Language::Frame`: Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl` with its `PsiBuilderImpl`, Java's
  `PsiBuilderImpl`), each a builder with all its buffers (lex types and starts, markers, free lists, production,
  entries, the join/newline/adapter stacks);
- the file's and the relexed `Lexed`;
- the tree being built.

`frames[..depth]` are the builders still binding; the ones past `depth` are finished and are reset in place
(`PsiBuilderImpl::reset`: cleared, marker 0 pushed) for the next file or chameleon, so no builder is moved or rebuilt
per chameleon: moving them (a builder is 584 bytes, with its buffers 992) through the language's parse and the frame
stack cost about 5.5 KB of copies per chameleon. Nothing is global or thread-local.

PSI builds blocks, lambdas, doc comments and markdown links as chameleons: each is reparsed in a fresh builder over
its own text. The port does the same, in place:

1. `bind` is a resumable state machine that stops at each chameleon.
2. The driver leaves the suspended frame where it is, parses the chameleon on the next frame (reusing the file's
   tokens when they cover it exactly, relexing otherwise), binds it, then resumes.

No native recursion is involved, so chameleon nesting takes heap, not stack, and node
ids come out strictly in preorder.

### Nesting limit (`builder::NESTING_LIMIT`)

Within one builder the parser is recursive descent, as the Java is: `(((…1)))` recursed once per level and a 12 KB file
of parentheses overflowed a 2 MB stack, which aborts the whole process. PSI has no defined result there (kotlinc
overflows too), so the port defines one, and keeps PSI's tree below it.

- **What is counted.** `PsiBuilderImpl::enter_nesting` counts a level at every recursive entry point: an expression
  (`parsePrefixExpression`, which every expression recursion passes), a type (`parseTypeRefContents`), a member
  declaration (`parseMemberDeclaration`), a type argument list (`tryParseTypeArgumentList`), an annotation and a
  parenthesized type. A chameleon's builder starts at 4 levels over where its block or lambda was collapsed
  (`MarkerData::depth`), so frames are bounded too; body-less trees keep that depth per unexpanded body for
  `parse_body`.
- **The limits.** 1000 levels; 32 nested type argument lists per builder, because `a<a<…` tries one at every `<` and
  rolls it back (a list that reaches 32 also makes every list starting before that token too deep, so each `<` costs
  constant time on average); 10 nested annotations and parenthesized types per builder, because a rolled-back attempt parses
  them and the parse after it parses them again (`@A(@A(…))`, `((…) -> A) -> A`), which doubles the work per level:
  measured on c796e73, 14 levels of `@A(` took 7.3 ms and 1000 did not finish. Stack per level, measured with a stack
  pointer probe and by bisecting the depth that overflows 1 MB without a limit: 127 B (unary minus) to 559 B
  (nested `object`), 382 B for parentheses; random mixes of 42 forms stayed below 320 B. 1000 levels of the worst
  form fit 640 KB, not 512 KB. The community files reach 62 levels, 5 type argument lists and 4 reparsed
  constructs.
- **Past a limit** the construct is an error element "Code is too deeply nested" over the rest of it: from the
  current token to the first token that closes a bracket opened before it (`(`, `[`, `{`, `${`, quotes), at least one
  token. The table of these closers is made once per builder when first needed, so the error costs constant time,
  also inside an attempt that is rolled back; the construct around it then closes normally. A chameleon past the limit
  is not parsed: its node gets one such error with its tokens as leaves.
- **Frames.** A suspended frame drops the tokens `bind` has passed once they are at least 4096 and half of what it
  holds (`drop_bound_tokens`, keeping the first token of any collapsed marker still ahead), so nested chameleons hold
  O(tokens) memory, not O(depth · tokens).

`tests/robustness.rs` parses 22 forms nested 100 000 deep on a 1 MB stack, and every golden with limits 1 to 13;
the gate runs the community files and the mutations with limit 3 (see Contract).

The limits are `Language` constants (`NESTING_LIMIT`, `TYPE_ARGUMENTS_LIMIT`, `REPARSED_LIMIT`), whose defaults are
Kotlin's. A construct parsed twice that reaches its limit also makes every such construct starting before that token
too deep (`reparsed_too_deep_before`, as for type argument lists): otherwise the constructs re-parsed after the
rollback of the one around them, each reaching the limit again from a shallower start, cost a polynomial of the
input (Java's `@A(f(@A(f(…` grew as n³ with a limit of 3); below the limits nothing changes.

### Java nesting (`java-syntax`; `Java::NESTING_LIMIT`)

The Java parsers recurse once per level as Kotlin's do: 20 000 nested parentheses in an initializer overflowed the
stack. They count on the same `psi_builder` machinery (`SyntaxTreeBuilder::enter_nesting`), and past a limit a
construct is the same error element over the rest of it (`java_parser_util::too_deeply_nested`, which returns the
error's marker as the construct the caller parsed).

- **What is counted.** Every recursion cycle passes one counted function: an expression
  (`tryParseWithPrecedenceAtMost`) and a unary operand (`parseUnary`'s operand of a prefix operator or a cast, the
  recursion that does not pass it); a statement (`parseStatement`); a declaration (`DeclarationParser.parse`, once
  the token may start one: members, local and anonymous classes, enum constant bodies); an array initializer (an
  annotation's recurses through `parseAnnotationValue` without an expression); a type (`parseTypeInfo`); a type
  argument list (`parseReferenceParameterList` at `<`, `Nesting::TypeArguments`); an annotation
  (`Nesting::Reparsed`); a pattern (`parseTypeOrRecordPattern`, record patterns); an inline doc tag (`parseDataItem` at
  `{@`). `ModuleParser` and `JShellParser` have no cycle of their own. A code block is a body and starts its frame at
  `NESTING_CHAMELEON` over where it was collapsed; a doc comment and its reference holders start at 0 with the parser's
  limit (`Chameleon::builder`), as a KDoc does: they hold no body, so a frame is bounded by its own tokens.
- **The limits.** 1200 levels: IntelliJ's own test `Parenth2.java` (in the community corpus and the goldens) nests
  1001 parenthesized expressions, 1010 levels; the next deepest community file reaches 174 (`NestedLambda.java`), the
  deepest production file 102. A level takes at most 652 bytes of stack in a release build (typed and parenthesized
  lambdas, enum constant bodies; measured by bisecting the depth that overflows 1 MB without a limit, over 45 forms:
  127 B for unary operators and casts, 461 B for parentheses, 588 B for calls), and the deepest forms at the limit
  need 772 KB (bisected stack size), so a parse fits 1 MB. 32 type argument lists (the community files reach 4). 5
  annotations (the community files reach 4): an annotation in an expression is parsed up to 4 times per parse of the
  one around it (as a type's annotations, as the expression's, in a cast and in a lambda's parameters, each rolled
  back), so `@A(f(@A(f(…))))` took 3 times the work per level and 16 levels 12.6 s; at 5, an annotation takes at most
  4^5 = 1024 times its own work, as Kotlin's 2^10.
- **Block ends in constant time.** `braceMatchingBuilder` (every class body) and `parseBlockLazy` (every code block)
  find the `}` that closes a block with a loop over the tokens, which nested class bodies in one builder and nested
  bodies across frames ran once per level: 16 000 nested classes took 2.3 s, anonymous classes 5.8 s.
  `PsiBuilderImpl::balanced_block_end` answers what that loop finds. Both callers ask right after consuming the `{`,
  so a builder over a slice of the file's tokens reads the `}` that closes it from the file's brace pairs (one stack
  pass over the file's tokens per file, `builder::brace_pairs`, shared by every such builder; not when a brace of
  the builder was remapped). Otherwise it counts the raw tokens while the builder has
  counted fewer than it has tokens, then makes a table of every token's answer in one pass (the stack pass of the
  too-deep closers); a body deeper than `SHARED_BLOCK_DEPTH` (64) takes the file's table, made once per file
  (`Language::BLOCK_BRACES`), instead of counting its own tokens again in every frame. A remapped bracket makes the
  table stale up to it (Java's parsers remap no brace). The marker and rollback around the loop stay, so the builder's
  state after it is the Kotlin's; `parseBlockLazy` ends at the `}` or at the first token from the stopping offset on,
  as the loop does.
- **Markdown labels.** `parseMarkdownReferenceOrLink` scans from each `[` to the `]` that balances it, remapping the
  DOC_SPACEs on the way: `/// [[[…]]]` ran it at every `[` over the rest, 16 000 brackets 1.2 s. The `]` comes from
  `balanced_block_end_at` over the doc brackets, each DOC_SPACE is remapped once (`remap_label_spaces`: nested labels
  lie inside the one around them), and the bracket counts, only compared with each other and with 1, from whether a
  `[` is inside.

`tests/robustness.rs` (Java) parses 80 recursive forms nested 100 000 deep on a 1 MB stack (each under 110 ms; two
more at 10 000: see "Open items"), 44 chains and lists of 100 000 operators or elements (each under 70 ms), 40 random
mixes of 26 expression forms 3000 deep, random bytes and fragments, and every golden with limits 1 to 13 (lossless,
`parse_body` equal to the full parse's subtree); the gate runs `--check-bodies` on the Java corpora at the default
limit and at 3.

### Tree (`psi-builder` `src/tree.rs`)

A tree is three columns over the node ids, in preorder:

| column | type | holds |
|---|---|---|
| `kinds` | `u16` | `ElementType::index` (tokens below 256, composite types from 256), `LEAF` bit, `UNEXPANDED` bit |
| `ranges` | `[u32; 2]` | start and end byte |
| `links` | `[u32; 2]` | parent and subtree end |

That is 18 bytes a node. The tree also holds the text (`Arc<[u8]>`) and an `errors` table sorted by id, which is
allocated only when the file has errors.

- **Navigation.** The subtree of `id` is `id..subtree_end`, the first child is `id + 1` when the subtree is not empty,
  and the next sibling is `subtree_end` while that is inside the parent's subtree. `last_child` walks the children.
- **Building.** The tree is built in the `Parser`'s buffer, one record per node (one push), and copied into exactly
  sized columns, so there is no slack.
- **Leaves.** Whitespace and comments are leaves wherever PSI's binders put them. No string is stored per leaf: a
  leaf's text is its range.

### Consumer API

AGENTS.md, "The consumer contract", is the normative list. In short:

- `Parser::new()` once per thread, then `parse(text, FileKind)` or `parse_with_options` for each file. `Parser` is
  `Send`; `Tree` is `Send + Sync` and has no lifetime. A const assertion checks both.
- `descendants(id)` is an id range and `preorder(id)` a linear scan with depth, so "every `CALL_EXPRESSION` under X"
  reads one column.
- `leaves()` gives `(TokenKind, Range)` in document order. It is the only exact source for highlighting: soft
  keywords are remapped, and `?.`/`?:`/`!!` joined, by the parser, not the lexer.
- `ParseOptions { expand_bodies: false }` leaves each block and lambda as a childless node marked `is_unexpanded`. Doc
  comments are still expanded. `parse_body(id)` returns that body's exact subtree as its own `Tree`; the gate checks
  this on every community body.

## Decisions

| decision | why | evidence |
|---|---|---|
| A faithful port, one function per Java method, MarkerProduction semantics | Byte equality with PSI on every input, including error recovery; a diff against the Java reads line by line | gate: 0 of 55 501 community files differ, and 0 of 3000 for each parse-fuzz corpus |
| Speed from representation, allocation and layout, never from parser decisions | Equality stays provable by the gate and the counters | all 25 counters identical across batches 2 and 3; in batch 1 only the cursor-call counts (token cache) and the lazy-block counts (brace skip) moved, as intended |
| Chameleons reparsed exactly as PSI does, not parsed eagerly in the file builder | Not equivalent: a fresh builder resets the newline and join stacks, the edge binders and the error dedup at the chameleon boundary | `reusable_tokens`' DANGLING_NEWLINE case; the brace skip removes the repeated work instead |
| In-place expansion over a heap frame stack | Preorder ids (1.03M preorder breaks under the old LIFO worklist) without native recursion | walk 3.62 → 2.23 ns/node |
| The production as a linked list over marker nodes, made into an array for binding | `precede` and the other middle edits in constant time with the array's entry order, so the same tree; a gap buffer or chunks would still need the lexeme search | gate unchanged; 100 000-operator chains 14-45 ms (`+`: 4.67 s → 22 ms); parse 72.7 → 74.2 MB/s |
| Java's nesting on the same counter, with its own limits (`Language` constants) | One mechanism and one error for both languages; Java's stack per level (652 B) and `Parenth2.java` (1010 levels) want 1200, its annotations (parsed up to 4 times per level) 5 | gate unchanged, now with Java's `--check-bodies` at limits 1200 and 3; 80 forms nested 100 000 deep on 1 MB |
| Block ends counted, then tabulated, the table shared by deep frames | `braceMatchingBuilder` and `parseBlockLazy` scanned to the `}` once per level; a table made by every builder cost that again in every frame of a deep chain | 16 000 nested classes 2.3 s → 1.9 ms; 100 000 nested `try` with a table per builder 515 ms (92 % making tables), counted then shared 38 ms |
| A nesting counter that degrades to an error element, not a bigger stack or `stacker` | Deterministic on every platform and build, bounded memory, no `unsafe`; the dump is PSI's wherever PSI itself parses | gate unchanged; 100 000 levels of 22 forms on 1 MB in at most 110 ms; parse 74.4 → 75.0 MB/s (noise) |
| The too-deep error over the rest of the construct, in constant time | One token at a time cost `2^10` times the rest of the file inside doubled annotations; a scan to the closer cost O(n) per rolled-back `<` attempt | `@A(` × 100 000: 4.2 ms |
| A per-thread `Parser`, no thread_local | Allocation churn dominated multi-threaded scaling | allocations per pass 10.7M → 179 779; 16 threads 684 → 917 MB/s (f3aa172 → a99fb45) |
| Copy the tree out exactly sized, instead of reserving `tokens*15/8+16` | Same CPU time, less memory | user CPU 3.12 vs 3.13 s; 11.32 vs 12.70 MB of tree per source MB |
| Pair columns `ranges`/`links`, not five `u32` columns | Same 18 B/node, 3 pushes instead of 5 | parse 70.0 → 71.5 MB/s (9 of 9 rounds), linear walk 1.68 → 1.59 ns/node |
| `psi-builder` generic over a `Language` trait, monomorphized; not `dyn`, not language-neutral numeric ids | The Java port reuses the builder, tree and driver; Kotlin keeps its enums and every parser call site (`kotlin_syntax` aliases the generic types), and the compiled Kotlin parse is the one it was | gate unchanged, all 25 counters identical; 1 thread B/A 0.985 before forcing `skip_whitespace` inline and leaving `getJoinedTokenType` outlined, after it paired-round medians 0.991 and 0.997 at load 6-14 (see the split's measurement) |
| No `unsafe`, no C toolchain, no large dependency tree | Consumers build it on every platform; any byte sequence must parse | `unsafe_code = "forbid"`; the only dependency is the optional `stats_alloc` |

### Libraries

A maintained crate is preferred to reinventing it when it is measurably as fast or faster and the output stays
byte-identical. None qualified. The measurements are from the survey prototypes and batch 2, on the community corpus:

| crate | verdict | evidence |
|---|---|---|
| phf 0.14 | rejected | `phf_map!` 47.5 MB/s against 47.7 for a byte `match` (base 41.0); it adds a build-time macro crate |
| simdutf8 0.1.5 | rejected | +0.6 %, noise; not validating at all (`get_token_bytes`) gave +4.8 %; whole-file validation is under 0.25 % of a parse |
| memchr 2.8.3 | deferred | table plus memchr against table alone: lexer only 608 vs 595 MB/s (+2.2 %), end to end +1.6 %, below noise; `memchr3` cannot take the 4 string-part needles |
| logos 0.16.1 | rejected | cannot `yypushback(n)` (9 rules), has no state stack with `lBraceCount`, and its Unicode classes differ from JFlex's Java tables; a simplified ceiling grammar was about 2 % faster than ours on the lexer, which is under 10 % of a parse |
| rowan 0.17, cstree 0.14, biome_rowan 0.7 | rejected | building their green trees on top of ours cost +44 % (rowan, per-file cache), +60 % (rowan, shared cache) and +70 % (cstree) of a whole parse; walks 2.9× (rowan) and 4.5× (cstree) slower; rowan tokens are `&str` (no BAD_CHARACTER bytes) and its `SyntaxNode` is `!Send` |
| bumpalo, typed-arena | rejected | lifetime-bound references conflict with an owned `Send + Sync` tree |
| id-arena, la-arena | rejected | the same as `Vec` plus `u32` ids |
| smallvec | rejected | reusing buffers in the `Parser` removes the same allocations |
| lasso, rustc-hash, foldhash | rejected | no crate has a hash map, no per-leaf `String`; interning is the consumer's choice |
| bitflags, fixedbitset | rejected | `TokenSet { [u64; 8] }` with const constructors covers tokens and node types; the `at_set` cost was the keyword loop and `from_utf8` |
| mimalloc 0.1.52 | consumer-side only | needs C. Before buffer reuse: +12 % at 8 threads, +15 % at 16. After it (a99fb45): +3.8 % at 8, +3.1 % at 16 (load 8-15); 1 thread not measured |
| stats_alloc 0.1.10 | kept, optional | the `alloc-stats` feature, profiling only |

## Measurements

The machine is an Apple M5 Max shared with other sessions. Every comparison alternates its binaries. A one-thread run
reports the summed parse time; a multi-threaded run reports wall time. The corpus is the community list (55 501 files,
172.91 MB).

### Current, c796e73, 2026-11-01

There were 3 alternating rounds. In each round `ksdump --bench --repeat 3` (or `ts-bench … 3`) ran with 1 thread,
then tree-sitter-kotlin with 1 thread, then 16 threads, then `--walk`. The figures are medians over rounds of each
run's median. Load was 24 in round 1, 19 in round 2 and 10 in round 3. Round 1 ran at about half speed for both
parsers.

| measure | rounds | median |
|---|---|---|
| parse, 1 thread | 38.6 / 75.3 / 75.4 MB/s | **75.3 MB/s** |
| tree-sitter-kotlin 0.25.10 (C core), 1 thread, same files | 11.9 / 20.7 / 19.9 MB/s | **19.9 MB/s** (ours 3.8×) |
| parse, 16 threads | 863.5 / 965.2 / 951.3 MB/s | **951.3 MB/s** |
| preorder walk (kind, range, depth) | 2.19 / 2.15 / 2.14 ns/node | **2.15 ns/node**, 6.0 % of the parse |
| linear `descendants()` scan (kind, range) | 1.60 / 1.56 / 1.55 ns/node | **1.56 ns/node**, 4.3 % of the parse |
| body-less parse, 1 thread (load 4) | 142.3 / 141.1 / 140.0 MB/s | **141.1 MB/s**, 31 240 422 nodes |
| lexer alone (`--lex --bench`, load 4) | 480.0 / 477.4 / 476.8 MB/s | **477.4 MB/s** |

The tree is deterministic, so these are single values:

- 63 742 904 nodes;
- 7.64 MB of tree per source MB (text included);
- 20.7 bytes a node;
- 368.6 nodes per source KB;
- the largest tree is 3.4 MB;
- the body-less tree is 4.25 MB per source MB.

An earlier set of single runs at load 8-24 gave a 1-thread median of 44.5 MB/s for ours and 10.9 MB/s for
tree-sitter. Ours was 3.6-4.1× faster in every round.

The gate passed on c796e73 in 62 s.

### The workspace split (child of 26d5703), 2026-11-01

The crate split, `psi-builder` generic over `Language`, against 26d5703, both `ksdump --bench` on the community files:

- the 25 `--features counters` counters, the 63 742 904 nodes, the 7.64 MB of tree per source MB, the `--walk`
  checksum and the body-less 31 240 422 nodes are identical;
- the first split binary measured 1 thread 75.1 → 74.0 MB/s (B/A 0.985, 11 alternating rounds, A faster in 10, load
  3.3-5.1). Its code had changed in two places: `skip_whitespace`, inlined before, was a call on every cursor move,
  and `getJoinedTokenType`, a call before, was forced inline into every `tt()`. With `#[inline(always)]` on the first
  and no attribute on the second the code is within 1 KB of 26d5703's (697 064 vs 698 088 bytes of functions);
- that binary, alternating with 26d5703: 15 rounds at load 7.7-18 (median of per-round B/A 0.997), 21 rounds at load
  5.7-13.8 (69.4 vs 69.2 MB/s, median of per-round B/A 0.991). The machine was shared throughout; a quieter repeat is
  the open check;
- instructions retired of a whole `ksdump --bench` process (`/usr/bin/time -l`, file reading included), 5 runs each:
  26d5703 73.26-74.57 G, the split 73.48-74.69 G, equal within their run-to-run spread.

The gate passed on the split.

### The Java lexers and the parser skeleton (child of 000c6b3), 2026-11-01

- `ksdump --lex` against `tools/java-oracle/JavaLexOracle.java` (IntelliJ 271's java-syntax `JavaLexer`, and
  `JavaDocLexer` on every doc comment): 0 differing lines on the 68 263 community `.java` files (3 not UTF-8, both
  sides "unreadable"), the 19 116 golden mutations and 20 000 lexer fuzz inputs (`lex_fuzz.py`, seed `java-lex`);
  the 1036 golden inputs, untrimmed, equal too.
- `tests/lexer.rs`: 1036 of 1036 goldens have the leaf texts of their PSI dump (36 with tokens the parser joins, 55
  with relexed holders).
- Lexing the community `.java` files (215.75 MB) with both lexers and hashing every token: 5.2 s wall for
  `ksdump --lex`, 12.5 s for the Java oracle (one run each, file reads included, the oracle's JVM start too).
- Kotlin: `psi-builder`'s new parameters and methods change no Kotlin parse. Instructions retired of a whole
  `ksdump --bench` process, 3 runs each: before 73.59-74.67 G, after 74.64-74.70 G; `scripts/ab.sh`, 7 rounds at load
  9.5-17: per-round B/A 0.825-1.056, median 1.012 (too loaded to resolve 1 %).

The gate passed (including the Java lexer checks).

### The Java parsers (the merge of the declaration and statement lanes, child of 0f619c0), 2026-11-01

`DeclarationParser`, `FileParser`, `ModuleParser`, `ReferenceParser`, `PatternParser`, `JShellParser` (one lane),
`StatementParser`, `PrattExpressionParser`, `JavaDocParser` (the other), merged without conflicts; one method stays a
stub, `JavaParserUtil.stoppingBuilder(builder, condition)`, which no parser calls. Against IntelliJ 271's Java PSI:

- the goldens (`tests/goldens.rs`): 1036/1036 `.txt` and 1036/1036 `_node.txt`; `tests/doc_comments.rs`: 224/224
  doc comments of 219 goldens;
- the community `.java` files (`ksdump --node --out`): 68 260 of 68 263 dumped (3 not UTF-8, failures on both sides),
  0 differing files of either dump; 35.4 s wall, 557 MB peak RSS (one run, file writes included);
- the 19 116 golden mutations: 0 differing files of either dump;
- the 20 000 lexer fuzz inputs (`lex_fuzz.py`, seed `java-lex`, no cached PSI dumps; the oracle run once for this):
  parse without a panic, 19 989 equal, 11 differ in a dump label only (see "Open items").

The gate passed, with the community and mutation dumps of both kinds added to it.

### The Java dump labels of the lexer fuzz (child of 4631197), 2026-11-01

The 11 differing `java-lex` inputs were two labels. `PsiInlineDocTagImpl.toString()` concatenates a missing name
element as `null` (9 inputs, `/**{@ doc */`); `PsiPackageStatementImpl.getPackageName()` is
`JavaSourceUtil.getReferenceText`, which leaves out whitespace, comments and annotations and puts a space between two
leaves that would join into one identifier (2 inputs, `package pack1.@code pack2;`). The dump now ports both, and the
module name `PsiJavaModuleReferenceElementImpl.getReferenceText()` (the reference's children that are not whitespace or
comments; it walked into a doc comment's tokens before, no input had shown it). The oracle's dumps of the corpus
(both kinds, 5.8 s; 17 604 of 20 000 with a `PsiErrorElement`) are cached and gated: 0 differing files of either dump,
as for the 68 260 community files and the 19 116 golden mutations; goldens 1036/1036 `.txt` and `_node.txt`.

### Java parse throughput (child of 75baa59), 2026-11-02

`ksdump --bench` over the 68 263 community `.java` files (215.75 MB, `java-community-3caa0bf.list`, all parsed as files
at `JDK_28`; the 3 that are not UTF-8 parse as bytes), alternating in each round with tree-sitter-java 0.23.5 on
tree-sitter 0.25.10 (C core; `/tmp/ts-java-bench`, not committed: one `tree_sitter::Parser` per thread, the same files
with the same CRLF normalization read before timing, `parse(text, None)`, fat LTO). Each run is `--repeat 3` and gives
its median; one thread reports the summed parse time, 16 threads the wall time. 5 rounds of ours 1 thread,
tree-sitter 1 thread, ours 16 threads, tree-sitter 16 threads, ours body-less 1 thread. The machine was heavily shared:
load 14-37 (1-minute average at the start of each run), so these are lower bounds, not a quiet figure.

| measure | rounds | median |
|---|---|---|
| parse, 1 thread | 59.3 / 54.5 / 51.6 / 54.5 / 52.7 MB/s | **54.5 MB/s** |
| tree-sitter-java, 1 thread | 23.1 / 23.5 / 22.3 / 20.3 / 16.5 MB/s | **22.3 MB/s**; per-round ratio ours/tree-sitter 2.32-3.19, median **2.57×** |
| parse, 16 threads | 545.7 / 498.9 / 591.3 / 480.7 / 437.4 MB/s | **498.9 MB/s** |
| tree-sitter-java, 16 threads | 150.0 / 163.7 / 153.4 / 135.0 / 122.7 MB/s | **150.0 MB/s**; per-round ratio 3.05-3.85, median **3.56×** |
| body-less parse, 1 thread | 98.7 / 97.5 / 22.9 / 93.4 / 63.2 MB/s | **93.4 MB/s** (round 3 ran during a load spike) |

A single earlier pair at load 13.5: ours 62.2 MB/s 1 thread (`--repeat 3`), 680.0 MB/s 16 threads; tree-sitter-java
25.3 and 175.7 MB/s right after it at load 23. The deterministic figures:

- 85 270 521 nodes; 8.12 MB of tree per source MB; 20.6 bytes a node; 395.2 nodes per source KB; the largest tree
  9.3 MB;
- body-less: 38 558 344 nodes, 4.23 MB per source MB;
- preorder walk 2.12 ns/node (5.2 % of the parse), linear scan 1.63 ns/node (4.0 %), `--walk` checksum
  `c7e0ef63cac45006` (one run at load 13.5);
- tree-sitter-java reports an error (`has_error`) in 1898 files; PSI has a `PsiErrorElement` in 990.

Kotlin's 75.3 MB/s (c796e73) was measured at load 10-24 with one round at load 4 and is not comparable to these
figures; a Java profile and a quiet round are open items. The bench change itself leaves the Kotlin parse alone:
instructions retired per Kotlin parse round (half the difference of `--repeat 3` and `--repeat 1` processes, two runs
each) 53.4 and 54.1 G on 75baa59, 54.7 and 53.9 G with the generic bench; `scripts/ab.sh`, 9 rounds at load 37-62,
gave B/A 0.909 with per-round values from 9.0 to 58.6 MB/s, which resolves nothing.

The gate passed.

### The Java nesting guard (child of 61b4d1d), 2026-11-02

- Corpus, `scripts/ab.sh` against 61b4d1d, one thread, 11 alternating rounds at load 12-20 over the community `.java`
  files: 56.0 → 60.4 MB/s (median B/A of the rounds 1.08, B faster in all 11). The gain is `parseBlockLazy`'s
  count of the raw tokens in place of its loop of `tokenType` and `advanceLexer` over every code block. Kotlin, 9
  rounds at load 10-19: 69.1 → 69.1 MB/s (median of the per-round B/A 0.993; round 8's B ran at 39.6 during a spike).
- Trees: every dump of the gate's Java and Kotlin corpora unchanged; the deepest community `.java` file
  (`Parenth2.java`) reaches 1010 levels, 4 type argument lists and 4 annotations.
- Adversarial inputs on a 1 MB thread, `tests/robustness.rs`, at load 12-20: 80 forms nested 100 000 deep in 1.5 ms
  (`!a`) to 102 ms (`new A() { void f() { g(`, a chain of bodies); before the guard the parser's own recursions overflowed
  a 1 MB stack below 10 000 levels, and on a 1 GB stack 16 000 levels took up to 8.0 s (enum constant bodies) and
  `@A(f(` 16 levels 22 s.
  44 chains and lists of 100 000 in 2.0 ms (modifiers) to 65 ms (`a<b>c` × 100 000); `a < a < …` took 15 s for 16 000
  operators before (each `a` tried a type with every `<` after it). `/// [[[…]]]`: 100 000 brackets 8.9 ms, 55 s
  before.

The gate passed.

### History (1 thread / 16 threads, alternating A/B medians at each step)

| commit | change | 1 thread MB/s | 16 threads MB/s | other |
|---|---|---|---|---|
| 1c8bcfa | baseline | 39.0 | 455.5 | tree 15.86 MB per source MB, 43.0 B/node; 10.7M allocations per pass |
| 50ac0fe | byte-match keyword lookups | 39.2 → 46.2 | 474.7 → 552.2 | |
| efbfcbc | token text as bytes | 46.5 → 48.8 | 553.7 → 577.7 | |
| ed452bd | current-token cache, handle truncation cache | 48.1 → 52.0 | 561.8 → 582.7 | RawTokenType calls per token 12.21 → 1.68 |
| a7d433e | `#[inline(always)]` cursor chain | 51.4 → 61.2 | 583.9 → 674.3 | binary 734 → 1016 KB |
| f3aa172 | raw brace scan for lazy blocks | 61.7 → 63.7 | 683.0 → 698.0 | Advance per token 1.28 → 0.77 |
| 6871816 | per-thread `Parser` buffers | 62.4 → 67.8 | 682.5 → 833.7 | |
| bf052ef | bind from the production, 20-byte markers | 67.4 → 70.6 | 800.6 → 881.3 | |
| a99fb45 | tree copied out exactly sized | 71.7 → 72.4 | | 11.32 MB per source MB; 179 779 allocations per pass |
| 279585f … 7fc4e69 | in-place chameleons, preorder columns | 71.5 → 71.9 | | 7.64 MB per source MB; walk 3.62 → 2.23 ns/node |
| a405a03 | ASCII identifier table | 71.9 → 73.9 | 918.0 → 926.1 | lexer alone 441.3 → 507.7 MB/s |
| 36b3b17 | the nesting limit, frames dropping bound tokens | 74.4 → 75.0 (9 rounds, load 3.4-5.1) | | 0 trees changed; 100 000 levels of 22 forms parse on 1 MB |
| child of 36b3b17 | linked production list | 72.7 → 74.2 (11 rounds, load 4.7-10.3) | | `a+a+…` × 100 000: 4.67 s → 22 ms; 18 chains 14-45 ms |

End to end: 1c8bcfa → f3aa172 measured 39.0 → 63.4 MB/s (×1.63), and f3aa172 → a99fb45 measured 60.8 → 69.1 MB/s
(×1.14, with 16 threads 684.2 → 917.0). Today's 75.3 MB/s on one thread and 951 MB/s on 16 are about 1.9× and 2.1× the
1c8bcfa numbers. They are not an A/B against 1c8bcfa.

The lexer-alone figure today (477 MB/s at load 4) is below the 507.7 MB/s recorded for a405a03. The source is the
same, so this is machine state, not a regression that was measured.

### Profile after batch 1 (f3aa172, `scripts/profile.py`, 1 thread)

These are self-time shares, before the batch 2 and 3 changes:

- build_tree 16.6 %, KotlinLexer::advance 7.6 %, index_of 6.8 %, mark 6.1 %, plain_identifier_end 5.4 %;
- memmove 4.8 %, insert_leaves 3.8 %, create_leaf 3.2 %, FlexKDocLexer::advance 2.7 %;
- malloc, free and realloc 3.5 % together.

The token cursor is no longer the dominant cost. Batches 2 and 3 targeted the bind, marker and allocation items.
There is no line-level profile of the current code: `atos` returned no line info in that session.

## What remains

### Open items

- **Java speed.** One thread at 54.5 MB/s (load 14-37; 60.4 at load 12-20 after the nesting guard) against Kotlin's
  75.3 (load 4-24): Java has 395 nodes per source KB against Kotlin's 369 and no profile yet. Take one
  (`scripts/profile.py`) and a quiet round before speed work; the Java builder path has no `--features counters`
  output.
- **Java's rolled-back lambda parameters.** `(a<b>c(…` looks like a typed lambda: its parameter list is parsed, with
  PSI's error recovery to the `)` that ends it, then rolled back, at every level. So each level inside the nesting
  limit scans the rest of the text: 10 000 levels of `(a<b>c(` with `)) + 1` or `)) - 1` after take 0.8 s, 100 000
  levels 15-25 s (linear in the text, times about 300 levels). Bounding it needs the attempt to know where its list
  ends without parsing it; the recovery parses references there (and remaps tokens that stay remapped after the
  rollback), so it is left as PSI's.

- **The split's speed on a quiet machine.** Its A/B ran at load 6-18; repeat `scripts/ab.sh` against 26d5703 at load
  under 4 before attributing a later change's small delta.

- **Copies into chameleon frames.** Each chameleon still copies its tokens from the file's (suspended frames then
  drop what they have bound). Nested chameleons over T tokens copy O(depth · T): 100 000 nested `{ fun g() ` (250
  frames at the limit) take 99 ms on 36b3b17, 2026-11-01. Sharing the file's tokens would remove it (see "Remaining
  copies").
- **mimalloc for consumers.** Re-measure on the current code, 1 thread included, before recommending it. It was +3-4 %
  at 8 and 16 threads on a99fb45.
- **Remaining copies.** `lex_types` is copied into each chameleon builder (remap mutates it), and the text is copied
  into the tree's `Arc` per file. Sharing the file's token starts measured 0.999× and was reverted; revisit it
  together with frame memory.
- **Next profile.** The current code has not been profiled at line level. Take one before choosing further cursor
  work.

### Rejected

| idea | why |
|---|---|
| Parsing bodies eagerly in the file builder | Not equivalent to PSI's fresh chameleon builder (newline and join stacks, binders and error dedup at the boundary, "Expecting '}'" dedup at the last lexeme) |
| A brace-match table from the lexer | The raw `lex_types` scan is already a byte-compare loop; at most about 1 %, plus memory per token |
| A soft-keyword id per token (rust-analyzer's contextual kind) | `soft_keyword_by_text` is about 1 % of samples after the byte match |
| A `next_significant` table or a newline-before bit | The token cache shrank their targets; revisit only if a profile shows more than 3 %. A chameleon's token 0 needs a special case |
| Production tombstones instead of `Vec::remove` in drop | Superseded by the linked production, whose `drop` unlinks in constant time |
| rust-analyzer's `forward_parent` / a marker-system rewrite | Breaks the faithful MarkerProduction port for about 5 %; `rollbackTo` a preceding marker has no creation-order equivalent. The linked production keeps the port and removes the quadratic case |
| Biome-style trivia on tokens | PSI puts whitespace and comments as leaves where binders decide; the dump would differ |
| Ruff's lazy lexer with checkpoints | PsiBuilder needs the whole token array for lookahead, rollback by index and chameleon token reuse |
| oxc's 256-entry byte-handler table and pointer `byte_search` | Need `unsafe`; the safe part (the ASCII table) is built |
| Exact highlighting from the lexer alone | Soft keywords and `?.`/`?:`/`!!` come from the parser; a lexer-only pass is approximate only |
| Lazy expansion through interior mutability (`expand(&self)`) | Conflicts with `Send + Sync` and the preorder arena; `parse_body` returns separate trees instead |
| Parsing one file's chameleons in parallel | Files take milliseconds and consumers already parallelize across files; it breaks preorder construction |
| mimalloc inside the crate | Needs a C toolchain; it is the consumer's choice |
| Shared token starts for chameleons | Measured 0.999× (1 thread) and 0.995× (16 threads); a copy into a warm buffer costs as much as the extra arithmetic on each access |
