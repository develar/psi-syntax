using System.Linq;
class A
{
    static void Main()
    {
        object[] p = { 1, 2, 3 };
        var q = from x in p
                where x is int?
                select x;
    }
}