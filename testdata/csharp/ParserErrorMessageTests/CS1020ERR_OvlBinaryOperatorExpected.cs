
namespace x
{
    public class iii
    {
        public static implicit operator int(iii x)
        {
            return 0;
        }
        public static implicit operator iii(int x)
        {
            return null;
        }
        public static int operator ++(iii aa, int bb)    // change ++ to +
        {
            return 0;
        }
        public static void Main()
        {
        }
    }
}
