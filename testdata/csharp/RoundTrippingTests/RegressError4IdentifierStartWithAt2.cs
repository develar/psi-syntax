public class A
        {
            public int @__namespace = 0;

            class @public // ok
            {
                private void M(int @int) { } // error
            }
            internal class @private // error
            {
            }
        }