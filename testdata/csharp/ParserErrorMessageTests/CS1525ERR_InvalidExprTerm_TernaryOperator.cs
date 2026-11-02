class Program
{
    static void Main(string[] args)
    {
        int x = 1;
        int y = 1;
        int s = true ?  : y++; // Invalid
        s = true ? x++ : ; // Invalid
        s =   ? x++ : y++; // Invalid
    }
}
