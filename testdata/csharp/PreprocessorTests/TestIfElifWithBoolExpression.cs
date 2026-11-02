#define True1
#define True2

#define False1
#undef False1
#undef False1
#undef False2

using System;

public class Test
{
    public static int Main(string [] args)
    {
		int i = 6;

 #if True1 == true
		i--;
 #endif

#if False1 == false
		i--;
#endif

# if false
	#error #elif True2 == True1
#elif True2 == True1
		i--;
#else
	#error #else #elif True2 == True1
 #endif

#if (True1 != false) && ((False1) == False2) && (true || false)
		i--;
# else
	#error #if (True != false) && ((False1) == False2) && (true || false)
# endif

#if ((true == True1) != (false && true))
		i--;
#else
#error ((true == True1) != (false && true))
#endif

#if !(!(!!(true))) != false
		i--;
#else
#error !(!(!!(true))) != false
#endif

		return(i > 0 ? 1 : 0);
    }
}