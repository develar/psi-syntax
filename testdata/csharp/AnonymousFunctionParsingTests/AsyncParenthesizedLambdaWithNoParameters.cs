
using System;
using System.Threading.Tasks;

public class C
{
    public static int a;
    void M1()
    {
        Func<Task<int>> v = async () => a;
    }
}