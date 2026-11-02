
class Program
{
    static void Main(string[] args)
    {
        string target = "t1";
        switch (target)
        {
        label1:
        case "t1":
            goto label1;
        }
    }
}
