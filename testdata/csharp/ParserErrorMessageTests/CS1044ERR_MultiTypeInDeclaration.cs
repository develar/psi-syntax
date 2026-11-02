
using System;

// two normal classes...
public class Res1 : IDisposable
{
    public void Dispose()
    {
    }
    public void Func()
    {
    }
    public void Throw()
    {
    }
}

public class Res2 : IDisposable
{
    public void Dispose()
    {
    }
    public void Func()
    {
    }
    public void Throw()
    {
    }
}

public class Test
{
    public static int Main()
    {
    using (    Res1 res1 = new Res1(), 
        Res2 res2 = new Res2())
        {
            res1.Func();
            res2.Func();
        }
    return 1;
    }
}
