
class C
{
    void M()
    {
        System.Func<int, int> x = [My(nameof(parameter))] delegate { return 1; }
    }
}

public class MyAttribute : System.Attribute
{
    public MyAttribute(string name1) { }
}
