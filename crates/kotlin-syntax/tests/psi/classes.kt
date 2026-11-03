class C : Base?, (Int) -> Unit, I by d, B(1), a.b.Q<T>()
enum class E(val i: Int) { A(1), B { }, C }
enum interface EI
fun interface FI { fun f() }
data class D(val a: Int) { constructor() : this(1) }
value class V(val v: Int)
inline class W(val w: Int)
inner class N
annotation class An
