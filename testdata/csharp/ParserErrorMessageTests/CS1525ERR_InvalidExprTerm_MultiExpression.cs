class Program
{
    static void Main(string[] args)
    {
        int x = 1;
        int y = 1;
        int s = true ? x++, y++ : y++; // Invalid
        s = true ? x++ : x++, y++; // Invalid
    }
}
