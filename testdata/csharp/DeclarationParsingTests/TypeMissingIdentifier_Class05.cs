public partial class (string name)
    : Base<T>(name)
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public class <U> : Base<U>
        where U : List<U>;

    public void Method2() { }
}
public partial class <T>(int length)
    : Base<T>(length)
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public class <U> : Base<U>
        where U : List<U>;

    public void Method2() { }
}