
public class Base {
    public virtual void M(ref int X) {
    }
}
public class Derived : Base {
    public override void M(ref readonly int X) {
    }
}