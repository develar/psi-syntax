fun f(x: (@A Foo)?) {}
val (@A Foo)?.r: Int get() = 1
typealias T1 = (Foo)
typealias T2 = Foo??
typealias T3 = (Foo?)?
val fv: (B.() -> Unit)? = null
fun h(): @A
val v: a.`b c`.D<Int>.E<String>? = null
fun (Int?).g() {}
