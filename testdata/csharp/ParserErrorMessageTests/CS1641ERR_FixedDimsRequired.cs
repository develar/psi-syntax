
unsafe struct S 
{
    fixed int [] ia;  // CS1641
    fixed int [] ib[];  // CS0443
};
class Test
{
    public static int Main()
    {
        return 1;
    }
}
