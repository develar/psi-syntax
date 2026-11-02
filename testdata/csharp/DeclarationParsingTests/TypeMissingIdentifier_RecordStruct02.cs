public record struct (ImmutableArray<T> Array)
    : IStruct<T>
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public partial record struct ;
    public struct <U> : IStruct<U>
        where U : List<U>;

    public void Method2() { }
}
file record struct <T> : IStruct<T>
{
    file record struct <U>(IEnumerable<U> Enumerable);
    file struct <U> { }
    file struct : IStruct;
    file struct;
}