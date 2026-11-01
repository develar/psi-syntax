# kotlin-syntax: design

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

Speed work therefore changes only three things, none of which changes a parser decision:

- how the cursor and the markers are represented;
- when and what is allocated;
- the layout of the finished tree.

Before and after each such change, the 25 `--features counters` counters (cursor calls, markers, rollbacks, lexed,
reused and skipped tokens) are compared. The token cache was meant to move only the raw cursor-call counts, and the
brace skip only the calls, markers and tokens skipped inside lazy blocks. Every other change left all 25 identical.

## Pipeline

```
text ─ lexer::tokenize_into ─> Lexed {types, starts}      (file tokens, Parser-owned buffers)
     ─ PsiBuilderImpl + KotlinParsing ─> production        (markers, one Rust fn per Java method)
     ─ begin_bind: balance_white_spaces, bind ─> TreeData   (Parser-owned columns, preorder)
         └─ at each chameleon (BLOCK, LAMBDA, KDoc, markdown link): suspend the frame, parse it, bind, resume
     ─ copy out ─> Tree {Arc<[u8]>, kinds, ranges, links, errors}   (exactly sized, Send + Sync)
```

### Lexer (`src/lexer.rs`, `src/kdoc/lexer.rs`)

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

### Builder (`src/builder.rs`)

`PsiBuilderImpl` is ported with IntelliJ's semantics:

- the `MarkerProduction` order;
- rollback to a marker's production entry, freeing markers from the last entry back;
- the global `lastErrorIndex` error dedup;
- the edge binders, `remapCurrentToken` and `tokens_to_restore_on_rollback`.

Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl` adds the newline-mode and token-join stacks on top. The adapters
(`TruncatedSemanticWhitespaceAwarePsiBuilder`, `ForByClause`) are a stack inside the builder, seen through a handle,
`SemanticWhitespaceAwarePsiBuilder`.

What the port adds to the hot path:

- **A current-token cache** in `PsiBuilderImpl`: `tt_raw`, `tt_joined` and `tt_valid`. `current_types` holds the
  `getJoinedTokenType` body.
  - The cache lives in `PsiBuilderImpl`, not in the semantic-whitespace layer, because `Marker` methods mutate
    `PsiBuilderImpl` directly.
  - It is invalidated wherever `current_lexeme` moves or `lex_types` changes: `advance_lexer`, `raw_advance_lexer`
    (and `advance_to_eof`), `rollback_to`, `remap_current_token` and `skip_balanced_block`.
  - `skip_whitespace` keeps it valid, because it only steps over whitespace.
- **The handle caches** `top_trunc` and `min_eof` (the smallest non-negative `eof_position`). It sets them in
  `handle()`, `reborrow()` and `wrapped()`. This is sound by the borrow checker: `wrapped(self)` consumes the handle
  and `reborrow` freezes its parent, so the adapter prefix cannot change while the handle is alive.
- **Token text is compared as bytes** (`get_token_bytes`, which returns `b"?:"` and `b"?."` for joined tokens) in
  `at`, `at_set`, `expect` and the keyword helpers. Only a test calls `get_token_text`, so `str::from_utf8` is off the
  hot path.
- **`#[inline(always)]` on the cursor chain** (`tt`, `_at`, `token_matches`, `_at_set`, `at`, the handle's and the
  impl's `get_token_type`/`eof`).
- **`skip_balanced_block`.** When no `Truncated` adapter is active, `advance_balanced_block` counts `LBRACE`/`RBRACE`
  over the raw `lex_types`, instead of the Java loop that joins `?.`/`?:`/`!!` and creates their collapse markers.
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
- **`production` and `production_lexemes`** are the list made into arrays by `prepare_light_tree`, each entry's
  lexeme read from its marker; `balance_white_spaces` rewrites the lexemes and `bind` walks the arrays.
- **`bind` walks the production directly**, with an explicit stack and no `prepareLightTree` links. It reproduces
  `prepareLightTree`'s rules:
  - any done closes the current marker at its own done lexeme;
  - the stack starts as `[root]`, so entries after the root's done become root children;
  - error items inside collapsed markers count for the global dedup, although their entries are skipped;
  - open markers close innermost first.

### Parser driver and chameleons (`src/tree.rs`)

`Parser` owns every buffer a parse needs:

- `BuilderBuffers`: lex types, markers, free lists, production, the join/newline/adapter stacks;
- the file's and the relexed `Lexed`;
- the starts scratch;
- the tree columns;
- the stack of suspended builder frames, with spare buffers for them.

A builder takes these buffers in `new()` (clears them, pushes marker 0) and gives them back in `release()`. Nothing is
global or thread-local.

PSI builds blocks, lambdas, doc comments and markdown links as chameleons: each is reparsed in a fresh builder over
its own text. The port does the same, in place:

1. `bind` is a resumable state machine that stops at each chameleon.
2. The driver pushes the suspended frame on a heap stack, parses the chameleon (reusing the file's tokens when they
   cover it exactly, relexing otherwise), binds it, then resumes.

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

### Tree (`src/tree.rs`)

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
- **Building.** The tree is built in the `Parser`'s columns and copied into exactly sized `Vec`s, so there is no slack.
  The pair columns cost 3 pushes per node; 5 separate columns cost 5.
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
| A nesting counter that degrades to an error element, not a bigger stack or `stacker` | Deterministic on every platform and build, bounded memory, no `unsafe`; the dump is PSI's wherever PSI itself parses | gate unchanged; 100 000 levels of 22 forms on 1 MB in at most 110 ms; parse 74.4 → 75.0 MB/s (noise) |
| The too-deep error over the rest of the construct, in constant time | One token at a time cost `2^10` times the rest of the file inside doubled annotations; a scan to the closer cost O(n) per rolled-back `<` attempt | `@A(` × 100 000: 4.2 ms |
| A per-thread `Parser`, no thread_local | Allocation churn dominated multi-threaded scaling | allocations per pass 10.7M → 179 779; 16 threads 684 → 917 MB/s (f3aa172 → a99fb45) |
| Copy the tree out exactly sized, instead of reserving `tokens*15/8+16` | Same CPU time, less memory | user CPU 3.12 vs 3.13 s; 11.32 vs 12.70 MB of tree per source MB |
| Pair columns `ranges`/`links`, not five `u32` columns | Same 18 B/node, 3 pushes instead of 5 | parse 70.0 → 71.5 MB/s (9 of 9 rounds), linear walk 1.68 → 1.59 ns/node |
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
| lasso, rustc-hash, foldhash | rejected | `src/` has no hash map, no per-leaf `String`; interning is the consumer's choice |
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
