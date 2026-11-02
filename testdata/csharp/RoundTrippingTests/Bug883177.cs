
unsafe
struct Test
{
    public fixed int i[1];
    byte* myIntBuf;
    static int Main()
    {
        int retval = 0;
        Test t = new Test();
        t.i[0] = 0;
        t.myIntBuf = (byte*) t.i;
        if (*t.myIntBuf !=0)
            retval = 1;
        if (retval != 0)
            System.Console.WriteLine("Failed.");
        return retval;
    }
}
