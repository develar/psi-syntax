class C
{
    void M()
    {
        // bad visibility modifier
        protected void localFunc() {}
    }
    void M2()
    {
        typing
        protected void localFunc() {}
    }
    void M3()
    {
    // Ambiguous between local func with bad modifier and missing closing
    // brace on previous method. Parsing currently assumes the latter
    protected void M4() {}
}