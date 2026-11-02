
namespace x
{
    public class ii
    {
        int i
        {
            get
            {
                return 0;
            }
        }
    }
    public class a 
    {
        public static ii operator ii(a aa) // replace ii with explicit or implicit
        {
            return new ii();
        }
        public static void Main()
        {
        }
    }
}
