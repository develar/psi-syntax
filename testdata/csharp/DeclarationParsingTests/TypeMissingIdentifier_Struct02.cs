public struct : 
{
    public int Property { get; set; }
    public void Method1() { }

    public struct : IStruct { }
    public struct : IStruct<int>;

    public void Method2() { }
}