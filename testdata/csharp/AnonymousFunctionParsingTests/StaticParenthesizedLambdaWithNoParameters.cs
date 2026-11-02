
using System;

public class C
{
    public static int a;
    void M1()
    {
        Func<int> v = static () => a;
    }
}