class Program
{
    static int Main(string[] args)
    {
        int s = true ? goto lab1: goto lab2; // Invalid
    lab1:
        return 0;
    lab2:
        return 1;
    }
}
