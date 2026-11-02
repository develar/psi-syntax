
public class Production
{
    public Production()
    {
        ((VoidDelegate)delegate
        {
            this.someType.Iterate(delegate(object o)
            {
                System.Console.WriteLine(((BoolDelegate)delegate { return object.Equals(o, this.epsilon); })());
            });
        })();
    }
}
