# kotlin-syntax

A lossless Kotlin lexer and parser in Rust, ported from the Kotlin compiler's own (`Kotlin.flex`, `KotlinParsing`,
`KotlinExpressionParsing`, IntelliJ's `PsiBuilder`). Its tree prints exactly what Kotlin's
`DebugUtil.psiToString(file, true, false)` prints. See AGENTS.md for the design and the oracles.

Licensed under the Apache License, Version 2.0; see NOTICE for the code it ports.
