namespace N
{
    class C
    {
    }

    // Will get pulled into C
    void Method()
    {
    }

    // Not currently pulled in.  But could consider it in the future.
    private class T
    {
    }

    // Will get pulled into T
    void Method2()
    {
    }
}