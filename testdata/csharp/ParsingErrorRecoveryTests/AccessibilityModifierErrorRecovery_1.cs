class C
{
    void M()
    {
        // bad visibility modifier
        public void localFunc() {}
    }
    void M2()
    {
        typing
        public void localFunc() {}
    }
    void M3()
    {
    // Ambiguous between local func with bad modifier and missing closing
    // brace on previous method. Parsing currently assumes the latter
    public void M4() {}
}