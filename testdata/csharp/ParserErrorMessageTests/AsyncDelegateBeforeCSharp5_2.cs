
class C
{
    static void Main()
    {
        Func<int, Task<int>> f = async delegate (int x) { return x; };
    }
}
