class Goo
{
    int L { get; } = 12; // auto property initializer

    int M() => 12; // expression-bodied method

    int N => 12; // expression-bodied property

    int this[int a] => a + 1; // expression-bodied indexer
    
    public static int operator +(Goo a, Goo b) => null; // expression-bodied operator
    
    public static explicit operator bool(Goo a) => false; // expression-bodied conversion operator

    void P(object o)
    {
        try {
        } catch (Exception ex) when (ex.ToString() == null) { // exception filter
        }

        var s = o?.ToString(); // null propagating operator
        var x = $"hello world";
    }
}