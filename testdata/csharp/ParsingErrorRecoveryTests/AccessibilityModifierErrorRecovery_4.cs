class C
{
    void M()
    {
        // bad visibility modifier
        private void localFunc() {}
    }
    void M2()
    {
        typing
        private void localFunc() {}
    }
    void M3()
    {
    // Ambiguous between local func with bad modifier and missing closing
    // brace on previous method. Parsing currently assumes the latter
    private void M4() {}
}