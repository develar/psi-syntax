public struct <T>(string length)
    : IStruct<T>
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public struct <U> : IStruct<U>
        where T : List<U>;

    public void Method2() { }
}