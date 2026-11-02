
class C1
{
    static void Test(int arg1, (byte, byte) arg2)
    {
        (int, int)? t1 = new(int, int)?();
        (int, int)? t1a = new(int, int)?((1,1));
        (int, int)? t1b = new(int, int)?[1];
        (int, int)? t1c = new(int, int)?[] {(1,1)};

        (int, int)? t2 = default((int a, int b));

        (int, int) t3 = (a: (int)arg1, b: (int)arg1);

        (int, int) t4 = ((int a, int b))(arg1, arg1);
        (int, int) t5 = ((int, int))arg2;

        List<(int, int)> l = new List<(int, int)>() { (a: arg1, b: arg1), (arg1, arg1) };

        Func<(int a, int b), (int a, int b)> f = ((int a, int b) t) => t;
        
        var x = from i in "qq"
                from j in "ee"
                select (i, j);

        foreach ((int, int) e in new (int, int)[10])
        {
        }
    }
}
