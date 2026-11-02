class C
{
    void M()
    {
        // bad visibility modifier
        internal void localFunc() {}
    }
    void M2()
    {
        typing
        internal void localFunc() {}
    }
    void M3()
    {
    // Ambiguous between local func with bad modifier and missing closing
    // brace on previous method. Parsing currently assumes the latter
    internal void M4() {}
}