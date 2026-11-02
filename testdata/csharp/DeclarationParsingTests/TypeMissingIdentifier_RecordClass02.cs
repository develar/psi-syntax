public record class (ImmutableArray<T> Array)
    : BaseRecord<T>(Array)
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public partial record class ;
    public class <U> : Base<U>
        where U : List<U>;

    public void Method2() { }
}
file record class <T> : Base<T>
{
    file record class <U>(IEnumerable<U> Enumerable);
    file class <U> { }
    file class : Base;
    file class;
}