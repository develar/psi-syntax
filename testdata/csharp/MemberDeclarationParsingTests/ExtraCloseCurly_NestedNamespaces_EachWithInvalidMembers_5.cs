namespace N1
{
    namespace N2
    {
        record struct Type
        {
        }

        // Should go into type
        private void M()
        {
        }

        // Should become the close curly of Type initially.
        }

        // Will not go into Type because N2 will initially end prior to this.
        // After M joins into Type and NT finishes, this will be a child of
        // N1.  We could think about merging this into Type at some point.
        private void N()

        // Should become the close curly of N1
        }
    }
}