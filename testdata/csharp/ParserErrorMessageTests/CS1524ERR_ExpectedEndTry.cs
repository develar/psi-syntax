using System;
namespace nms
{
    public class mine
    {
        private static int retval = 5;
        public static int Main()
        {
        try {
            Console.WriteLine("In try block, ready to throw.");
            sizeof (throw new RecoverableException("An exception has occurred"));
            }
        return retval;
        }
    };
}
