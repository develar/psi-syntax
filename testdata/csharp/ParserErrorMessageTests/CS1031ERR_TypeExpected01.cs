
namespace x
{
    interface ii
    {
        int i
        {
            get;
        }
    }
    public class a 
    {
        public operator ii(a aa)
        {
            return new ii();
        }
    }
}
