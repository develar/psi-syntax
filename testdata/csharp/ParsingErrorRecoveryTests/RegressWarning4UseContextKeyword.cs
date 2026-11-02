class TestClass
{
    int partial { get; set; }
    static int Main()
    {
        TestClass tc = new TestClass();
        tc.partial = 0;
        return 0;
    }
}
