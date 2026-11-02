class C
{
    void M()
    {
        var v = [A] () =>
        {
            // This attribute, within an expression of a lambda, should be totally fine to parse.  It
            // is not within an attribute argument itself.
            var y = [B] () => { };
        };
    }
}