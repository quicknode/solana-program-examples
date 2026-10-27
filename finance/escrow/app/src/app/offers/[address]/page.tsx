import { isAddress } from "@solana/kit";
import type { Metadata } from "next";
import Link from "next/link";
import { OfferDetail } from "@/components/offer-detail";

type OfferPageProps = { params: Promise<{ address: string }> };

export const metadata: Metadata = { title: "Offer" };

const OfferPage = async ({ params }: OfferPageProps) => {
  const { address } = await params;
  if (!isAddress(address)) {
    return (
      <div className="empty-state">
        <h1>That is not an offer address</h1>
        <p>
          <span className="address">{address}</span> is not a Solana address.
        </p>
        <Link className="button" href="/">
          See the open offers
        </Link>
      </div>
    );
  }
  return <OfferDetail offerAddress={address} />;
};

export default OfferPage;
