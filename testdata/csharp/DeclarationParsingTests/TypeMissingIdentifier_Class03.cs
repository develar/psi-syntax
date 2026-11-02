public class : Base<T>
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public class <U> : Base<U>
        where U : List<U>;

    public void Method2() { }
}