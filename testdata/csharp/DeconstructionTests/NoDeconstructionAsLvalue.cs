
class C
{
    void M()
    {
        var (x, y) = e; // ok, deconstruction declaration
        var(x, y); // ok, invocation
        int x = var(x, y); // ok, invocation
    }
}