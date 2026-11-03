# psi-syntax moved into accio

psi-syntax (the Kotlin, Java and C# lexers and parsers ported from the Kotlin compiler, IntelliJ and Roslyn) now lives
in [JetBrains/accio](https://github.com/JetBrains/accio) as
[`psi-syntax/`](https://github.com/JetBrains/accio/tree/main/psi-syntax), its crates members of accio's workspace, its
history replayed there as a line (JetBrains/accio#208). This repository is archived at the commit the move imported
(25dac59) plus this note; consumers pin a revision of JetBrains/accio:

```toml
kotlin-syntax = { git = "https://github.com/JetBrains/accio", rev = "<a revision of accio's main>" }
```
