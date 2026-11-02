public partial record struct ()
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public partial record struct ;
    public struct <U> : IStruct<U>
        where U : List<U>;

    public void Method2() { }
}
public record struct : IStruct<T>
{
    partial struct;
}