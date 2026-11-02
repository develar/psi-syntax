public partial record class ()
    where T : List<T>
{
    public int Property { get; set; }
    public void Method1() { }

    public partial record class ;
    public class <U> : Base<U>
        where U : List<U>;

    public void Method2() { }
}
public record class : Base<T>
{
    partial class;
}