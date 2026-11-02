
using System;
using System.Threading.Tasks;

public class C
{
    void M1()
    {
        Func<int, Task<int>> v = async static async => async;
    }
}