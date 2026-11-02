class C
{
    void M()
    {
        // This is an attributed lambda *within* a collection expression.  This is fine.
        var v = [A([B]() => {})];
    }
}