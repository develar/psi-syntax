// Every KtSimpleNameExpression: labels and operation references have a referenced name too.
fun f() {
    run foo@{ return@foo }
    loop@ for (i in 1..2) break@loop
    val x = a to b
    x += 1
    y!!
    -z
    a !in b
    c ?: d
    e as? F
    this@C.x
    super<A>@B.f()
    if (g is H) {}
}

// An explicit backing field's type and initializer.
class K {
    val items: List<String>
        field = mutableListOf()
    val more: List<Int>
        field: MutableList<Int> = mutableListOf()
}

// Context, lambda and function type parameters; named arguments.
context(ctx: Ctx)
fun p(block: (s: String) -> Unit) {
    block("")
    q(name = 1) { it -> }
}
