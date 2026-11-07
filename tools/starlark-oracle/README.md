# Starlark lexer oracle

This runner invokes the Bazel plugin's `StarlarkIndentingLexer` from the pinned IntelliJ `languages` jar. Its hash/count
output has the same format as `ksdump --lang starlark --lex`, so the complete token streams can be compared without
materializing a dump for every token.

Build the plugin jar in idea-1:

```sh
bazel build //plugins/bazel/intellij.bazel.languages:languages
mkdir -p out/starlark-oracle
cp out/bazel-bin/plugins/bazel/intellij.bazel.languages/languages.jar out/starlark-oracle/languages.jar
```

List the Starlark files in the checkout, then run both lexers over that exact list:

```sh
accio files '*' --project ~/projects/idea-1 \
  --scope '**/BUILD' --scope '**/BUILD.bazel' --scope '**/WORKSPACE' --scope '**/WORKSPACE.bazel' \
  --scope '**/WORKSPACE.bzlmod' --scope '**/MODULE.bazel' --scope '**/REPO.bazel' --scope '**/*.bazel' \
  --scope '**/*.bzl' --scope '**/*.star' --scope '**/*.BUILD' --scope '!**/*.bazel/**' \
  --plain --limit 50000 > target/starlark.list

cargo run --release -p ide-oracle --bin starlark-oracle -- \
  --plugin-jar ~/projects/idea-1/out/starlark-oracle/languages.jar \
  lex --lines target/starlark.intellij.tsv -- --list target/starlark.list --root ~/projects/idea-1
cargo run --release -p ksdump -- --lang starlark --lex \
  --list target/starlark.list --root ~/projects/idea-1 > target/starlark.rust.tsv
cmp target/starlark.intellij.tsv target/starlark.rust.tsv

cargo run --release -p ide-oracle --bin starlark-oracle -- \
  --plugin-jar ~/projects/idea-1/out/starlark-oracle/languages.jar \
  psi --list target/starlark.list --root ~/projects/idea-1 --out target/starlark.intellij
cargo run --release -p ksdump -- --lang starlark --check target/starlark.intellij \
  --list target/starlark.list --root ~/projects/idea-1 --out target/starlark.psi-diff
```

The runner refuses a plugin jar whose SHA-256 differs from `languages.sha256`. Token dumps for a focused case can be
requested by adding `--dump` after `--root` to both lexer commands.

Generate a reproducible lexer-fuzz corpus from the idea-1 `.bzl` sources and compare both lexers on it:

```sh
cargo run --release -p ide-oracle --bin starlark-oracle -- \
  lex-fuzz ~/projects/idea-1/plugins/bazel target/starlark-oracle/fuzz 1000 starlark-lex
cargo run --release -p ide-oracle --bin starlark-oracle -- \
  --plugin-jar ~/projects/idea-1/out/starlark-oracle/languages.jar \
  lex --lines target/starlark-oracle/fuzz.intellij.tsv -- \
  --list target/starlark-oracle/fuzz/list --root target/starlark-oracle/fuzz
cargo run --release -p ksdump -- --lang starlark --lex \
  --list target/starlark-oracle/fuzz/list --root target/starlark-oracle/fuzz > target/starlark-oracle/fuzz.rust.tsv
cmp target/starlark-oracle/fuzz.intellij.tsv target/starlark-oracle/fuzz.rust.tsv
```