
using System;
static class Test
{
    static int Main()
    {
        Func<int,int> f1 = (x,) => 1;
        Func<int,int, int> f2 = (y,) => 2;
        return 1;
    }
}
