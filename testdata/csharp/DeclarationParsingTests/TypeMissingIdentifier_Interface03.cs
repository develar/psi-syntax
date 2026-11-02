public interface : IBase<T>
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public interface <U> : IBase<U>
        where T : List<U>;

    public void Method2() { }
}