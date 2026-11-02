class C
{
    void M()
    {
        // We won't recognize this as a lambda because we'll bail out from default-parameter parsing
        // when we see the `[` after the `=`.
        var v = (X x = [A] () => {}) => { };
    }
}