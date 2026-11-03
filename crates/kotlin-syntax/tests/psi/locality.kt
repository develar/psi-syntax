class K { val m = 1; fun f() { val local = 2; class L { val n = 3 }; fun lf() {}; val anon = fun() {} } }
val top = 1
object Ob { val o = 1 }
fun h() = object { val lit = 1 }
enum class En { A { val e = 1 } }
class Pc(val pc: Int)
val Int.ext get() = this
var s: Int = 0
    get() = field
    set(value) { field = value }
val byd by lazy { 1 }
