
class Test
{
    public static int Main()
    {
        long l = 25l;   // CS0078
        ulong n1 = 1lu;   // CS0078
        ulong n2 = 10lU;   // CS0078
        System.Console.WriteLine("{0}+{1}+{2}", l, n1, n2);
        return 0;
    }
}
